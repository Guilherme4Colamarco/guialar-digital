//! Backend Windows: DNS via PowerShell (`-EncodedCommand`) e elevação por UAC.

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;
use std::process::Command;

use crate::config::{
    BackendKind, ConnectionBackup, DnsStateManifest, CLOUDFLARE_IPV4, CLOUDFLARE_IPV6,
};
use crate::sysutil::{encode_powershell, is_safe_ps_alias};

/// Prefixo: saída UTF-8 e erro fatal (um cmdlet que falha deve falhar o processo).
const PS_PREFIX: &str =
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ErrorActionPreference='Stop'; ";

fn powershell(script: &str) -> Command {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
    ])
    .arg(encode_powershell(&format!("{PS_PREFIX}{script}")));
    cmd
}

fn powershell_output(script: &str) -> Result<String> {
    let out = powershell(script)
        .output()
        .context("Falha ao executar PowerShell")?;
    if !out.status.success() {
        bail!(
            "PowerShell falhou ({:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_powershell(script: &str) -> Result<()> {
    powershell_output(script).map(|_| ())
}

fn is_admin() -> bool {
    powershell_output(
        "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
    )
    .map(|o| o.trim().eq_ignore_ascii_case("true"))
    .unwrap_or(false)
}

/// Se não é administrador, reabre o programa com UAC rodando `subcomando` e
/// devolve `true` (trabalho feito na janela elevada). `false` = já é admin.
pub fn elevate_if_needed(subcomando: &str) -> Result<bool> {
    if is_admin() {
        return Ok(false);
    }
    let exe = std::env::current_exe().context("não foi possível localizar o executável")?;
    let exe = exe.to_string_lossy().replace('\'', "''");
    println!("🔐 Confirme a janela de permissão do Windows (UAC)...");
    run_powershell(&format!(
        "$p = Start-Process -FilePath '{exe}' -ArgumentList '{subcomando}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
    ))
    .context("a permissão de administrador foi negada ou a operação falhou")?;
    Ok(true)
}

pub fn apply_dns() -> Result<()> {
    if elevate_if_needed("internal-apply")? {
        return Ok(());
    }
    let interfaces = list_active_interfaces()?;
    if interfaces.is_empty() {
        bail!("Nenhuma interface de rede ativa encontrada no Windows");
    }
    let servers = format!(
        "'{}','{}','{}','{}'",
        CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1], CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]
    );
    let mut manifest = DnsStateManifest::new(BackendKind::WindowsPowerShell);
    for (alias, index) in &interfaces {
        if !is_safe_ps_alias(alias) {
            bail!("Alias de interface inválido retornado pelo Windows: {alias:?}");
        }
        manifest
            .connections
            .push(inspect_interface_state(*index, alias)?);
        run_powershell(&format!(
            "Set-DnsClientServerAddress -InterfaceIndex {index} -ServerAddresses @({servers})"
        ))?;
    }
    manifest.save_atomic()?;
    let _ = Command::new("ipconfig").arg("/flushdns").output();
    Ok(())
}

pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    if elevate_if_needed("internal-revert")? {
        return Ok(());
    }
    let mut falhas: Vec<String> = Vec::new();
    for conn in &manifest.connections {
        // O alias vem do manifesto em disco: pode ter sido adulterado.
        if !is_safe_ps_alias(&conn.id) {
            falhas.push(format!("alias inválido no manifesto: {:?}", conn.id));
            continue;
        }
        let script = if conn.was_dhcp_ipv4 {
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ResetServerAddresses",
                conn.id
            )
        } else if !conn.original_ipv4_dns.is_empty() {
            let ips: Vec<String> = conn
                .original_ipv4_dns
                .iter()
                .map(|ip| format!("'{ip}'"))
                .collect();
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ServerAddresses @({})",
                conn.id,
                ips.join(",")
            )
        } else {
            continue;
        };
        if let Err(e) = run_powershell(&script) {
            falhas.push(format!("{}: {e:#}", conn.id));
        }
    }
    if !falhas.is_empty() {
        bail!(
            "Falha ao reverter DNS em {} interface(s): {}",
            falhas.len(),
            falhas.join(" | ")
        );
    }
    DnsStateManifest::remove_manifest()?;
    let _ = Command::new("ipconfig").arg("/flushdns").output();
    Ok(())
}

fn list_active_interfaces() -> Result<Vec<(String, u32)>> {
    let out = powershell_output(
        "Get-NetIPInterface -AddressFamily IPv4 -ConnectionState Connected | Where-Object { $_.InterfaceAlias -notmatch 'Loopback|vEthernet' } | Sort-Object InterfaceIndex -Unique | ForEach-Object { \"$($_.InterfaceAlias)|$($_.InterfaceIndex)\" }",
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let (alias, idx) = l.trim().rsplit_once('|')?;
            Some((alias.to_string(), idx.parse().ok()?))
        })
        .collect())
}

fn inspect_interface_state(index: u32, alias: &str) -> Result<ConnectionBackup> {
    let out = powershell_output(&format!(
        "(Get-DnsClientServerAddress -InterfaceIndex {index} -AddressFamily IPv4).ServerAddresses -join ','"
    ))
    .context("Falha ao consultar DNS atual no Windows")?;
    let ips: Vec<Ipv4Addr> = out
        .trim()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    Ok(ConnectionBackup {
        id: alias.to_string(),
        was_dhcp_ipv4: ips.is_empty(),
        original_ipv4_dns: ips,
        original_ipv6_dns: Vec::new(),
        was_dhcp_ipv6: true,
    })
}

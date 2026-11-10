//! Backend Windows: DNS via PowerShell (`-EncodedCommand`) e elevação por UAC.

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;
use std::process::Command;

use crate::config::{current_mode, BackendKind, ConnectionBackup, DnsStateManifest};
use crate::sysutil::{encode_powershell, is_safe_ps_alias};

/// Prefixo: saída UTF-8 e erro fatal (um cmdlet que falha deve falhar o processo).
const PS_PREFIX: &str =
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ErrorActionPreference='Stop'; ";

/// `CREATE_NO_WINDOW`: nenhuma janela de console pisca ao chamar o PowerShell.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn powershell(script: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("powershell");
    cmd.creation_flags(CREATE_NO_WINDOW)
        .args([
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
    // SAFETY: função Win32 sem argumentos.
    unsafe { crate::win32::IsUserAnAdmin() != 0 }
}

/// Se não é administrador, reabre o programa com UAC (sem janela) rodando
/// `subcomando` e espera terminar. `true` = trabalho feito na instância elevada.
pub fn elevate_if_needed(subcomando: &str) -> Result<bool> {
    if is_admin() {
        return Ok(false);
    }
    crate::sysutil::OpLock::check_free()?;
    let exe = std::env::current_exe().context("não foi possível localizar o executável")?;
    let code = crate::win32::run_elevated_hidden(&exe.to_string_lossy(), subcomando)
        .map_err(anyhow::Error::msg)?;
    if code != 0 {
        bail!("a operação elevada falhou (código {code})");
    }
    Ok(true)
}

pub fn apply_dns() -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-apply --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    apply_core()
}

/// Troca o filtro mantendo o DNS original do manifesto (e tirando as políticas de navegador
/// quando o novo modo é só anúncios).
pub fn switch_mode(mut manifest: DnsStateManifest) -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-apply --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    let mode = current_mode();
    let servers = format!(
        "'{}','{}','{}','{}'",
        mode.ipv4()[0],
        mode.ipv4()[1],
        mode.ipv6()[0],
        mode.ipv6()[1]
    );
    for conn in &manifest.connections {
        if !is_safe_ps_alias(&conn.id) {
            bail!("alias inválido no manifesto: {:?}", conn.id);
        }
        run_powershell(&format!(
            "Set-DnsClientServerAddress -InterfaceAlias '{}' -ServerAddresses @({servers})",
            conn.id
        ))?;
    }
    if mode == crate::config::FilterMode::AdsOnly {
        crate::policy::revert_all()?;
    }
    manifest.mode = mode;
    manifest.save_atomic()?;
    flush_dns();
    Ok(())
}

/// DNS + políticas dos navegadores com um único prompt de UAC.
pub fn apply_all() -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-setup --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    apply_core()?;
    if current_mode() == crate::config::FilterMode::Family {
        crate::policy::install_all()?; // políticas de navegador só no modo familiar
    }
    Ok(())
}

/// Já elevado: grava o DNS e o manifesto.
fn apply_core() -> Result<()> {
    let _lock = crate::sysutil::OpLock::acquire()?;
    let interfaces = list_active_interfaces()?;
    if interfaces.is_empty() {
        bail!("Nenhuma interface de rede ativa encontrada no Windows");
    }
    let servers = format!(
        "'{}','{}','{}','{}'",
        current_mode().ipv4()[0],
        current_mode().ipv4()[1],
        current_mode().ipv6()[0],
        current_mode().ipv6()[1]
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
    flush_dns();
    Ok(())
}

pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    if elevate_if_needed("internal-revert")? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
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
            // ResetServerAddresses antes: senão o DNS IPv6 da Cloudflare ficaria na interface.
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{0}' -ResetServerAddresses; Set-DnsClientServerAddress -InterfaceAlias '{0}' -ServerAddresses @({1})",
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
    crate::policy::revert_all()?;
    flush_dns();
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
    let dhcp = ips.is_empty();
    let (ips, dhcp) = crate::config::sanitize_original_dns(ips, dhcp);
    Ok(ConnectionBackup {
        id: alias.to_string(),
        was_dhcp_ipv4: dhcp,
        original_ipv4_dns: ips,
        original_ipv6_dns: Vec::new(),
        was_dhcp_ipv6: true,
    })
}

/// `true` se alguma interface ativa ainda usa o DNS do filtro do GuiaLar.
pub fn cloudflare_dns_active() -> bool {
    powershell_output("(Get-DnsClientServerAddress -AddressFamily IPv4).ServerAddresses -join ','")
        .map(|o| {
            o.trim()
                .split(',')
                .filter_map(|s| s.trim().parse::<Ipv4Addr>().ok())
                .any(|ip| crate::config::is_filter_ipv4(&ip))
        })
        .unwrap_or(false)
}

/// Reversão de emergência sem manifesto: DNS automático em todas as interfaces.
pub fn revert_dns_fallback() -> Result<()> {
    if elevate_if_needed("internal-revert")? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    for (_, index) in list_active_interfaces()? {
        run_powershell(&format!(
            "Set-DnsClientServerAddress -InterfaceIndex {index} -ResetServerAddresses"
        ))?;
    }
    crate::policy::revert_all()?;
    flush_dns();
    DnsStateManifest::remove_manifest()?;
    Ok(())
}

fn flush_dns() {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("ipconfig")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

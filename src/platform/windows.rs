//! Backend Windows para configuração e reversão de DNS via PowerShell/Win32.

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;
use std::process::Command;

use crate::config::{
    BackendKind, ConnectionBackup, DnsStateManifest, CLOUDFLARE_IPV4, CLOUDFLARE_IPV6,
};

pub fn apply_dns() -> Result<()> {
    let mut manifest = DnsStateManifest::new(BackendKind::WindowsPowerShell);
    let active_interfaces = list_active_interfaces()?;

    if active_interfaces.is_empty() {
        bail!("Nenhuma interface de rede ativa conectada à internet encontrada no Windows");
    }

    let servers_arg = format!(
        "'{}','{}','{}','{}'",
        CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1], CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]
    );

    for (alias, if_index) in &active_interfaces {
        let backup = inspect_interface_state(*if_index, alias)?;
        manifest.connections.push(backup);

        let ps_cmd = format!(
            "Set-DnsClientServerAddress -InterfaceIndex {} -ServerAddresses @({})",
            if_index, servers_arg
        );
        run_powershell(&ps_cmd)?;
    }

    manifest.save_atomic()?;
    Ok(())
}

pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    for conn in &manifest.connections {
        let alias = &conn.id;

        if conn.was_dhcp_ipv4 {
            let ps_cmd = format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ResetServerAddresses",
                alias
            );
            let _ = run_powershell(&ps_cmd);
        } else if !conn.original_ipv4_dns.is_empty() {
            let ips: Vec<String> = conn
                .original_ipv4_dns
                .iter()
                .map(|ip| format!("'{}'", ip))
                .collect();
            let ps_cmd = format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ServerAddresses @({})",
                alias,
                ips.join(",")
            );
            let _ = run_powershell(&ps_cmd);
        }
    }

    DnsStateManifest::remove_manifest()?;
    Ok(())
}

fn list_active_interfaces() -> Result<Vec<(String, u32)>> {
    let ps_script = "Get-NetIPInterface -ConnectionState Connected | Where-Object { $_.InterfaceAlias -notmatch 'Loopback|vEthernet' } | Select-Object -Unique InterfaceAlias, InterfaceIndex | ForEach-Object { \"$($_.InterfaceAlias)|$($_.InterfaceIndex)\" }";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_script])
        .output()
        .context("Falha ao invocar PowerShell")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut interfaces = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.trim().split('|').collect();
        if parts.len() == 2 {
            let alias = parts[0].to_string();
            if let Ok(idx) = parts[1].parse::<u32>() {
                interfaces.push((alias, idx));
            }
        }
    }

    Ok(interfaces)
}

fn inspect_interface_state(if_index: u32, alias: &str) -> Result<ConnectionBackup> {
    let ps_script = format!(
        "(Get-DnsClientServerAddress -InterfaceIndex {} -AddressFamily IPv4).ServerAddresses -join ','",
        if_index
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
        .output()
        .context("Falha ao consultar DNS atual no Windows")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut original_ips = Vec::new();

    for ip_str in stdout.trim().split(',') {
        if let Ok(ip) = ip_str.trim().parse::<Ipv4Addr>() {
            original_ips.push(ip);
        }
    }

    let was_dhcp = original_ips.is_empty();

    Ok(ConnectionBackup {
        id: alias.to_string(),
        original_ipv4_dns: original_ips,
        original_ipv6_dns: Vec::new(),
        was_dhcp_ipv4: was_dhcp,
        was_dhcp_ipv6: true,
    })
}

fn run_powershell(script: &str) -> Result<()> {
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .status()
        .context("Falha ao executar comando PowerShell com privilégios")?;

    if !status.success() {
        bail!("Script PowerShell falhou com status {:?}", status.code());
    }
    Ok(())
}

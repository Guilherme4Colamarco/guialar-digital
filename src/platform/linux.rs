//! Backend Linux para configuração e reversão de DNS.
//!
//! Suporta 4 estratégias em ordem de prioridade:
//! 1. NetworkManager (`nmcli` por UUID da conexão ativa)
//! 2. systemd-resolved (drop-in em `/etc/systemd/resolved.conf.d/99-guialar.conf`)
//! 3. Runit / Openresolv (`resolvconf` ou serviços em `/var/service`)
//! 4. Arquivo direto `/etc/resolv.conf` (com backup e substituição atômica)

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::Path;
use std::process::Command;

use crate::config::{
    BackendKind, ConnectionBackup, DnsStateManifest, CLOUDFLARE_IPV4, CLOUDFLARE_IPV6,
};

/// Detecta qual subsistema de DNS está ativo no ambiente Linux.
pub fn detect_active_backend() -> BackendKind {
    // 1. Checa NetworkManager ativo (/run/NetworkManager existe ou `nmcli general status` responde)
    if Path::new("/run/NetworkManager").exists() || is_service_active_runit("NetworkManager") {
        if let Ok(output) = Command::new("nmcli").arg("general").arg("status").output() {
            if output.status.success() {
                return BackendKind::NetworkManager;
            }
        }
    }

    // 2. Checa systemd-resolved
    if Path::new("/run/systemd/resolve").exists() {
        return BackendKind::SystemdResolved;
    }

    // Fallback: se resolvconf existir no PATH
    if is_command_available("resolvconf") {
        return BackendKind::OpenResolvRunit;
    }

    // Fallback universal seguro
    BackendKind::DirectResolvConf
}

/// Reexecuta este programa elevado **uma única vez** (um só pedido de senha) com
/// `internal-apply` / `internal-revert`. Dentro de um AppImage usa o caminho do
/// AppImage (o ponto de montagem FUSE não é acessível ao root).
fn elevate_self(subcommand: &str) -> Result<()> {
    let appimage = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty());
    let exe = match &appimage {
        Some(a) => std::path::PathBuf::from(a),
        None => std::env::current_exe().context("não foi possível localizar o executável")?,
    };
    let prefix = crate::sysutil::current_privilege_prefix();
    let mut cmd = match prefix.split_first() {
        Some((tool, rest)) => {
            let mut c = Command::new(tool);
            c.args(rest);
            c
        }
        None => bail!("elevação indisponível"),
    };
    if appimage.is_some() {
        // root não acessa o mount FUSE do usuário: extrai e roda.
        cmd.args(["env", "APPIMAGE_EXTRACT_AND_RUN=1"]);
    }
    let status = cmd
        .arg(&exe)
        .arg(subcommand)
        .status()
        .context("Falha ao pedir permissão de administrador")?;
    match status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => {
            bail!(
                "a permissão de administrador foi negada (código {:?})",
                status.code()
            )
        }
        code => bail!("a operação com privilégios falhou (código {code:?})"),
    }
}

/// Aplica a proteção familiar conforme o backend detectado.
pub fn apply_dns() -> Result<()> {
    if !crate::sysutil::is_root() {
        return elevate_self("internal-apply");
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    let backend = detect_active_backend();
    let mut manifest = DnsStateManifest::new(backend.clone());

    match backend {
        BackendKind::NetworkManager => {
            apply_network_manager(&mut manifest)?;
        }
        BackendKind::SystemdResolved => {
            apply_systemd_resolved(&mut manifest)?;
        }
        BackendKind::OpenResolvRunit => {
            apply_openresolv_runit(&mut manifest)?;
        }
        BackendKind::DirectResolvConf => {
            apply_direct_resolv_conf(&mut manifest)?;
        }
        BackendKind::WindowsPowerShell => {
            bail!("Backend Windows chamado em ambiente Linux");
        }
    }

    manifest.save_atomic()?;
    Ok(())
}

/// Reverte as configurações para o estado original utilizando o manifesto.
pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    if !crate::sysutil::is_root() {
        return elevate_self("internal-revert");
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    match manifest.backend {
        BackendKind::NetworkManager => {
            revert_network_manager(manifest)?;
        }
        BackendKind::SystemdResolved => {
            revert_systemd_resolved()?;
        }
        BackendKind::OpenResolvRunit => {
            revert_openresolv_runit()?;
        }
        BackendKind::DirectResolvConf => {
            revert_direct_resolv_conf(manifest)?;
        }
        BackendKind::WindowsPowerShell => {
            bail!("Backend Windows chamado em ambiente Linux");
        }
    }

    DnsStateManifest::remove_manifest()?;
    Ok(())
}

// ============================================================================
// 1. NetworkManager Implementation (via nmcli)
// ============================================================================

fn apply_network_manager(manifest: &mut DnsStateManifest) -> Result<()> {
    let active_uuids = get_active_nm_connections()?;
    if active_uuids.is_empty() {
        bail!("Nenhuma conexão ativa do NetworkManager (ethernet/wifi) encontrada");
    }

    let ipv4_str = format!("{} {}", CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1]);
    let ipv6_str = format!("{} {}", CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]);

    for uuid in &active_uuids {
        let backup = inspect_nm_connection(uuid)?;
        manifest.connections.push(backup);

        run_privileged_command(
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv4.dns",
                &ipv4_str,
                "ipv4.ignore-auto-dns",
                "yes",
            ],
        )?;
        let _ = run_privileged_command(
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv6.dns",
                &ipv6_str,
                "ipv6.ignore-auto-dns",
                "yes",
            ],
        );
        run_privileged_command("nmcli", &["connection", "up", "uuid", uuid])?;
    }

    Ok(())
}

fn revert_network_manager(manifest: &DnsStateManifest) -> Result<()> {
    for conn in &manifest.connections {
        let uuid = &conn.id;
        if conn.was_dhcp_ipv4 {
            run_privileged_command(
                "nmcli",
                &[
                    "connection",
                    "modify",
                    uuid,
                    "ipv4.dns",
                    "",
                    "ipv4.ignore-auto-dns",
                    "no",
                ],
            )?;
        } else if !conn.original_ipv4_dns.is_empty() {
            let prev: Vec<String> = conn
                .original_ipv4_dns
                .iter()
                .map(|ip| ip.to_string())
                .collect();
            run_privileged_command(
                "nmcli",
                &["connection", "modify", uuid, "ipv4.dns", &prev.join(" ")],
            )?;
        }

        if conn.was_dhcp_ipv6 {
            let _ = run_privileged_command(
                "nmcli",
                &[
                    "connection",
                    "modify",
                    uuid,
                    "ipv6.dns",
                    "",
                    "ipv6.ignore-auto-dns",
                    "no",
                ],
            );
        }

        let _ = run_privileged_command("nmcli", &["connection", "up", "uuid", uuid]);
    }
    Ok(())
}

fn get_active_nm_connections() -> Result<Vec<String>> {
    let output = Command::new("nmcli")
        .args(["-t", "-f", "UUID,TYPE", "connection", "show", "--active"])
        .output()
        .context("Falha ao executar nmcli")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut uuids = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 {
            let uuid = parts[0];
            let conn_type = parts[1];
            if conn_type.contains("ethernet")
                || conn_type.contains("wireless")
                || conn_type.contains("wifi")
            {
                uuids.push(uuid.to_string());
            }
        }
    }
    Ok(uuids)
}

fn inspect_nm_connection(uuid: &str) -> Result<ConnectionBackup> {
    let output = Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "ipv4.dns,ipv4.ignore-auto-dns",
            "connection",
            "show",
            uuid,
        ])
        .output()
        .context("Falha ao inspecionar conexão do NetworkManager")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut was_dhcp = true;
    let mut original_ips = Vec::new();

    for line in stdout.lines() {
        if line.starts_with("ipv4.ignore-auto-dns:") {
            if line.contains("yes") {
                was_dhcp = false;
            }
        } else if line.starts_with("ipv4.dns:") {
            let val = line.trim_start_matches("ipv4.dns:").trim();
            for ip_str in val.split_whitespace() {
                if let Ok(ip) = ip_str.parse::<Ipv4Addr>() {
                    original_ips.push(ip);
                }
            }
        }
    }

    Ok(ConnectionBackup {
        id: uuid.to_string(),
        original_ipv4_dns: original_ips,
        original_ipv6_dns: Vec::new(),
        was_dhcp_ipv4: was_dhcp,
        was_dhcp_ipv6: true,
    })
}

// ============================================================================
// 2. systemd-resolved Implementation
// ============================================================================

fn apply_systemd_resolved(_manifest: &mut DnsStateManifest) -> Result<()> {
    let dropin_dir = Path::new("/etc/systemd/resolved.conf.d");
    if !dropin_dir.exists() {
        run_privileged_command("mkdir", &["-p", "/etc/systemd/resolved.conf.d"])?;
    }

    let conf_content = format!(
        "[Resolve]\nDNS={} {} {} {}\nFallbackDNS=1.1.1.1 8.8.8.8\n",
        CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1], CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]
    );

    let temp_file = "/tmp/99-guialar.conf.tmp";
    fs::write(temp_file, conf_content)?;
    run_privileged_command(
        "cp",
        &[temp_file, "/etc/systemd/resolved.conf.d/99-guialar.conf"],
    )?;
    let _ = fs::remove_file(temp_file);

    run_privileged_command("systemctl", &["restart", "systemd-resolved"])?;
    Ok(())
}

fn revert_systemd_resolved() -> Result<()> {
    let dropin_path = "/etc/systemd/resolved.conf.d/99-guialar.conf";
    if Path::new(dropin_path).exists() {
        run_privileged_command("rm", &["-f", dropin_path])?;
        let _ = run_privileged_command("systemctl", &["restart", "systemd-resolved"]);
    }
    Ok(())
}

// ============================================================================
// 3. Runit & Openresolv Implementation (Void Linux / runit services)
// ============================================================================

fn apply_openresolv_runit(_manifest: &mut DnsStateManifest) -> Result<()> {
    let conf = format!(
        "nameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
        CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1], CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]
    );

    let mut child = privileged_command("resolvconf", &["-a", "guialar"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("Falha ao iniciar resolvconf")?;
    child
        .stdin
        .take()
        .context("stdin do resolvconf indisponível")?
        .write_all(conf.as_bytes())?;
    let status = child.wait()?;

    if !status.success() {
        bail!("Falha ao aplicar DNS via resolvconf no runit");
    }
    Ok(())
}

fn revert_openresolv_runit() -> Result<()> {
    let status = privileged_command("resolvconf", &["-d", "guialar"]).status()?;

    if !status.success() {
        bail!("Falha ao remover interface guialar do resolvconf");
    }
    Ok(())
}

// ============================================================================
// 4. Direct /etc/resolv.conf Fallback (Atomic Replace)
// ============================================================================

fn apply_direct_resolv_conf(manifest: &mut DnsStateManifest) -> Result<()> {
    let resolv_path = Path::new("/etc/resolv.conf");
    if resolv_path.exists() {
        let current = fs::read_to_string(resolv_path).unwrap_or_default();
        manifest.previous_resolv_conf = Some(current);
    }

    let content = format!(
        "# Gerado pelo GuiaLar Digital - Protecao Familiar\nnameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
        CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1], CLOUDFLARE_IPV6[0], CLOUDFLARE_IPV6[1]
    );

    write_resolv_conf(&content)
}

/// Grava `/etc/resolv.conf` via `tee` com o conteúdo em stdin (sem arquivo temporário).
fn write_resolv_conf(content: &str) -> Result<()> {
    let mut child = privileged_command("tee", &["/etc/resolv.conf"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .context("Falha ao iniciar tee para /etc/resolv.conf")?;
    child
        .stdin
        .take()
        .context("stdin do tee indisponível")?
        .write_all(content.as_bytes())?;
    if !child.wait()?.success() {
        bail!("Falha ao gravar /etc/resolv.conf");
    }
    Ok(())
}

fn revert_direct_resolv_conf(manifest: &DnsStateManifest) -> Result<()> {
    if let Some(ref prev) = manifest.previous_resolv_conf {
        write_resolv_conf(prev)?;
    }
    Ok(())
}

// ============================================================================
// Helpers de Sistema
// ============================================================================

fn is_command_available(cmd: &str) -> bool {
    crate::sysutil::command_exists(cmd)
}

fn is_service_active_runit(service_name: &str) -> bool {
    let service_dir = Path::new("/var/service").join(service_name);
    if service_dir.exists() {
        if let Ok(output) = Command::new("sv").arg("status").arg(service_name).output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            return stdout.starts_with("run:");
        }
    }
    false
}

/// Comando já prefixado com a elevação adequada (nenhuma se já é root).
fn privileged_command(program: &str, args: &[&str]) -> Command {
    let prefix = crate::sysutil::current_privilege_prefix();
    let mut cmd = match prefix.split_first() {
        Some((tool, rest)) => {
            let mut c = Command::new(tool);
            c.args(rest).arg(program);
            c
        }
        None => Command::new(program),
    };
    cmd.args(args);
    cmd
}

fn run_privileged_command(program: &str, args: &[&str]) -> Result<()> {
    let status = privileged_command(program, args)
        .status()
        .with_context(|| format!("Falha ao executar {} com privilégios", program))?;
    if !status.success() {
        bail!(
            "Comando {} finalizou com erro ({:?})",
            program,
            status.code()
        );
    }
    Ok(())
}

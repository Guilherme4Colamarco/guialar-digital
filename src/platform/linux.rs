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

use crate::config::{current_mode, BackendKind, ConnectionBackup, DnsStateManifest};

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
    crate::sysutil::OpLock::check_free()?;
    let appimage = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty());
    let exe = match &appimage {
        Some(a) => std::path::PathBuf::from(a),
        None => std::env::current_exe().context("não foi possível localizar o executável")?,
    };
    let args_for = |tool: &str| -> Vec<std::ffi::OsString> {
        let mut v: Vec<std::ffi::OsString> = Vec::new();
        if tool == "sudo" {
            v.push("-n".into()); // nunca pede senha sem TTY
        }
        if appimage.is_some() {
            // root não acessa o mount FUSE do usuário: extrai e roda.
            v.extend(["env".into(), "APPIMAGE_EXTRACT_AND_RUN=1".into()]);
        }
        v.push(exe.clone().into_os_string());
        v.extend(subcommand.split_whitespace().map(std::ffi::OsString::from));
        v
    };
    let run = |tool: &str| -> Option<std::process::ExitStatus> {
        Command::new(tool).args(args_for(tool)).status().ok()
    };
    let have_pkexec = crate::sysutil::command_exists("pkexec");
    // 1º `sudo -n` (NOPASSWD/credencial em cache, funciona sem TTY nem polkit gráfico);
    // 2º `pkexec` (janela de senha do polkit para o usuário comum de desktop).
    // Sonda `sudo -n true`: só usa sudo se ele realmente dispensa senha; assim uma falha
    // do próprio comando não dispara um segundo pedido via pkexec.
    let sudo_ok = crate::sysutil::command_exists("sudo")
        && Command::new("sudo")
            .args(["-n", "true"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    if sudo_ok {
        let st = run("sudo").context("Falha ao executar sudo")?;
        return match st.code() {
            Some(0) => Ok(()),
            code => bail!("a operação com privilégios falhou (código {code:?})"),
        };
    }
    if !have_pkexec {
        bail!("elevação indisponível: instale pkexec (polkit) ou configure sudo");
    }
    let status = run("pkexec").context("Falha ao pedir permissão de administrador")?;
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

/// Extrai IPs de um texto qualquer (nameserver, `resolvectl dns`, `nmcli`): tokens que são
/// IP, sem sufixos `#sni` nem `%iface`. Duplicados saem uma vez.
pub fn parse_dns_ips(text: &str) -> Vec<std::net::IpAddr> {
    let mut out: Vec<std::net::IpAddr> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        // "nameserver 1.1.1.3" | "Link 2 (eth0): 1.1.1.3 1.0.0.3" | "IP4.DNS[1]:1.1.1.3"
        let body = match line.split_once(':') {
            Some((k, rest))
                if !k.contains(' ') || k.starts_with("Link") || k.starts_with("Global") =>
            {
                if k.starts_with("IP4")
                    || k.starts_with("IP6")
                    || k.starts_with("Link")
                    || k.starts_with("Global")
                {
                    rest
                } else {
                    line
                }
            }
            _ => line,
        };
        for tok in body.split(|c: char| c.is_whitespace() || c == ',') {
            let tok = tok.split(['#', '%']).next().unwrap_or("");
            if let Ok(ip) = tok.parse::<std::net::IpAddr>() {
                if !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
    }
    out
}

fn command_text(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// DNS em uso: resolv.conf + NetworkManager (`device show`) + systemd-resolved (`resolvectl`).
/// Ler só o resolv.conf não basta: com stub local ele mostra 127.0.0.53.
pub fn configured_dns() -> Vec<std::net::IpAddr> {
    let mut all = Vec::new();
    let mut add = |text: String| {
        for ip in parse_dns_ips(&text) {
            if !all.contains(&ip) {
                all.push(ip);
            }
        }
    };
    let resolv = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    add(resolv
        .lines()
        .filter(|l| l.trim_start().starts_with("nameserver"))
        .collect::<Vec<_>>()
        .join("\n"));
    if is_command_available("nmcli") {
        add(command_text(
            "nmcli",
            &["-t", "-f", "IP4.DNS,IP6.DNS", "device", "show"],
        ));
    }
    if is_command_available("resolvectl") {
        add(command_text("resolvectl", &["dns"]));
    }
    all
}

/// `true` se o DNS em uso ainda é o filtro do GuiaLar (resolv.conf ou perfil do NM).
pub fn cloudflare_dns_active() -> bool {
    let cf = |t: &str| {
        t.lines()
            .filter(|l| l.trim_start().starts_with("nameserver"))
            .any(|l| {
                l.split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<Ipv4Addr>().ok())
                    .is_some_and(|ip| crate::config::is_filter_ipv4(&ip))
            })
    };
    if fs::read_to_string("/etc/resolv.conf")
        .map(|t| cf(&t))
        .unwrap_or(false)
    {
        return true;
    }
    if is_command_available("nmcli") {
        if let Ok(uuids) = get_active_nm_connections() {
            for uuid in uuids {
                if let Ok(b) = inspect_nm_connection_raw(&uuid) {
                    if b.0.iter().any(crate::config::is_filter_ipv4) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Reversão de emergência: sem manifesto (corrompido/apagado) volta o DNS ao automático.
pub fn revert_dns_fallback() -> Result<()> {
    if !crate::sysutil::is_root() {
        return elevate_self("internal-revert");
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    match detect_active_backend() {
        BackendKind::NetworkManager => {
            for uuid in get_active_nm_connections()? {
                for fam in ["ipv4", "ipv6"] {
                    let dns = format!("{fam}.dns");
                    let ign = format!("{fam}.ignore-auto-dns");
                    run_privileged_command(
                        "nmcli",
                        &["connection", "modify", &uuid, &dns, "", &ign, "no"],
                    )?;
                }
                run_privileged_command("nmcli", &["connection", "up", "uuid", &uuid])?;
            }
        }
        BackendKind::SystemdResolved => revert_systemd_resolved()?,
        BackendKind::OpenResolvRunit => revert_openresolv_runit()?,
        BackendKind::DirectResolvConf | BackendKind::WindowsPowerShell => {
            // Sem backup do DNS anterior: só tira as linhas do filtro do GuiaLar.
            let atual = fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
            let limpo: Vec<&str> = atual
                .lines()
                .filter(|l| !l.contains("Gerado pelo GuiaLar"))
                .filter(|l| {
                    !(l.trim_start().starts_with("nameserver")
                        && crate::config::line_has_filter_ip(l))
                })
                .collect();
            write_resolv_conf(&(limpo.join("\n") + "\n"))?;
        }
    }
    DnsStateManifest::remove_manifest()?;
    Ok(())
}

/// Troca o filtro com a proteção ligada, mantendo o DNS original do manifesto.
pub fn switch_mode(mut manifest: DnsStateManifest) -> Result<()> {
    if !crate::sysutil::is_root() {
        return elevate_self(&format!(
            "internal-apply --mode {}",
            current_mode().cli_name()
        ));
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    match manifest.backend {
        BackendKind::NetworkManager => {
            let mode = current_mode();
            let v4 = format!("{} {}", mode.ipv4()[0], mode.ipv4()[1]);
            let v6 = format!("{} {}", mode.ipv6()[0], mode.ipv6()[1]);
            for conn in &manifest.connections {
                let u = &conn.id;
                run_privileged_command(
                    "nmcli",
                    &[
                        "connection",
                        "modify",
                        u,
                        "ipv4.dns",
                        &v4,
                        "ipv4.ignore-auto-dns",
                        "yes",
                    ],
                )?;
                let _ = run_privileged_command(
                    "nmcli",
                    &[
                        "connection",
                        "modify",
                        u,
                        "ipv6.dns",
                        &v6,
                        "ipv6.ignore-auto-dns",
                        "yes",
                    ],
                );
                run_privileged_command("nmcli", &["connection", "up", "uuid", u])?;
            }
        }
        BackendKind::SystemdResolved => apply_systemd_resolved(&mut manifest.clone())?,
        BackendKind::OpenResolvRunit => apply_openresolv_runit(&mut manifest.clone())?,
        BackendKind::DirectResolvConf => {
            // `apply_direct_resolv_conf` regravaria o backup: usa uma cópia descartável.
            apply_direct_resolv_conf(&mut manifest.clone())?;
        }
        BackendKind::WindowsPowerShell => bail!("Backend Windows chamado em ambiente Linux"),
    }
    manifest.mode = current_mode();
    manifest.save_atomic()?;
    Ok(())
}

/// Aplica a proteção conforme o backend detectado e o modo escolhido.
pub fn apply_dns() -> Result<()> {
    if !crate::sysutil::is_root() {
        return elevate_self(&format!(
            "internal-apply --mode {}",
            current_mode().cli_name()
        ));
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

    let ipv4_str = format!("{} {}", current_mode().ipv4()[0], current_mode().ipv4()[1]);
    let ipv6_str = format!("{} {}", current_mode().ipv6()[0], current_mode().ipv6()[1]);

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

fn inspect_nm_connection_raw(uuid: &str) -> Result<(Vec<Ipv4Addr>, bool)> {
    let b = inspect_nm_connection_unsanitized(uuid)?;
    Ok((b.original_ipv4_dns, b.was_dhcp_ipv4))
}

fn inspect_nm_connection(uuid: &str) -> Result<ConnectionBackup> {
    let b = inspect_nm_connection_unsanitized(uuid)?;
    let (ips, dhcp) = crate::config::sanitize_original_dns(b.original_ipv4_dns, b.was_dhcp_ipv4);
    Ok(ConnectionBackup {
        original_ipv4_dns: ips,
        was_dhcp_ipv4: dhcp,
        ..b
    })
}

fn inspect_nm_connection_unsanitized(uuid: &str) -> Result<ConnectionBackup> {
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
            for ip_str in val.split(|c: char| c.is_whitespace() || c == ',') {
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
        "[Resolve]\nDNS={} {} {} {}\n",
        current_mode().ipv4()[0],
        current_mode().ipv4()[1],
        current_mode().ipv6()[0],
        current_mode().ipv6()[1]
    );

    // Pelo stdin: nenhum arquivo previsível em /tmp.
    write_privileged_file(
        "/etc/systemd/resolved.conf.d/99-guialar.conf",
        &conf_content,
    )?;

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
        current_mode().ipv4()[0],
        current_mode().ipv4()[1],
        current_mode().ipv6()[0],
        current_mode().ipv6()[1]
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
        current_mode().ipv4()[0], current_mode().ipv4()[1], current_mode().ipv6()[0], current_mode().ipv6()[1]
    );

    write_resolv_conf(&content)
}

/// Grava `path` via `tee` (com privilégio) com o conteúdo em stdin, sem arquivo temporário.
fn write_privileged_file(path: &str, content: &str) -> Result<()> {
    let mut child = privileged_command("tee", &[path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("Falha ao iniciar tee para {path}"))?;
    child
        .stdin
        .take()
        .context("stdin do tee indisponível")?
        .write_all(content.as_bytes())?;
    if !child.wait()?.success() {
        bail!("Falha ao gravar {path}");
    }
    Ok(())
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

#[cfg(test)]
mod dns_parse_tests {
    use super::parse_dns_ips;

    fn s(v: &[&str]) -> Vec<std::net::IpAddr> {
        v.iter().map(|x| x.parse().unwrap()).collect()
    }

    #[test]
    fn le_resolv_conf_resolvectl_e_nmcli() {
        assert_eq!(
            parse_dns_ips("nameserver 1.1.1.3\nnameserver 2606:4700:4700::1113"),
            s(&["1.1.1.3", "2606:4700:4700::1113"])
        );
        assert_eq!(
            parse_dns_ips(
                "Global: 94.140.14.14#dns.adguard-dns.com\nLink 2 (eth0): 1.1.1.3 fe80::1%eth0"
            ),
            s(&["94.140.14.14", "1.1.1.3", "fe80::1"])
        );
        assert_eq!(
            parse_dns_ips("IP4.DNS[1]:10.0.2.3\nIP6.DNS[1]:2a10:50c0::ad1:ff\nIP4.DNS[2]:10.0.2.3"),
            s(&["10.0.2.3", "2a10:50c0::ad1:ff"])
        );
        assert!(parse_dns_ips("# nameserver 8.8.8.8\nsearch lan").is_empty());
    }
}

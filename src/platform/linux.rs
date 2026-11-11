//! Backend Linux para configuração e reversão de DNS.
//!
//! Suporta 4 estratégias em ordem de prioridade:
//! 1. NetworkManager (`nmcli` por UUID da conexão ativa)
//! 2. systemd-resolved (drop-in em `/etc/systemd/resolved.conf.d/99-guialar.conf`)
//! 3. Runit / Openresolv (`resolvconf` ou serviços em `/var/service`)
//! 4. Arquivo direto `/etc/resolv.conf` (com backup e substituição atômica)

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;

use super::runner::{Cmd, CommandRunner, In, Out, RealRunner};
use crate::config::{current_mode, BackendKind, ConnectionBackup, DnsStateManifest};

/// Detecta qual subsistema de DNS está ativo no ambiente Linux.
pub fn detect_active_backend() -> BackendKind {
    detect_active_backend_with(&RealRunner)
}

fn detect_active_backend_with(r: &dyn CommandRunner) -> BackendKind {
    // 1. Checa NetworkManager ativo (/run/NetworkManager existe ou `nmcli general status` responde)
    if r.path_exists("/run/NetworkManager") || is_service_active_runit(r, "NetworkManager") {
        if let Ok(output) = r.run(&Cmd::output("nmcli", &["general", "status"])) {
            if output.success() {
                return BackendKind::NetworkManager;
            }
        }
    }

    // 2. Checa systemd-resolved
    if r.path_exists("/run/systemd/resolve") {
        return BackendKind::SystemdResolved;
    }

    // Fallback: se resolvconf existir no PATH
    if r.command_exists("resolvconf") {
        return BackendKind::OpenResolvRunit;
    }

    // Fallback universal seguro
    BackendKind::DirectResolvConf
}

/// Reexecuta este programa elevado **uma única vez** (um só pedido de senha) com
/// `internal-apply` / `internal-revert`. Dentro de um AppImage usa o caminho do
/// AppImage (o ponto de montagem FUSE não é acessível ao root).
fn elevate_self(r: &dyn CommandRunner, subcommand: &str) -> Result<()> {
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
    let run = |tool: &str| -> Option<super::runner::Outcome> {
        r.run(&Cmd::status_os(tool, args_for(tool))).ok()
    };
    let have_pkexec = r.command_exists("pkexec");
    // 1º `sudo -n` (NOPASSWD/credencial em cache, funciona sem TTY nem polkit gráfico);
    // 2º `pkexec` (janela de senha do polkit para o usuário comum de desktop).
    // Sonda `sudo -n true`: só usa sudo se ele realmente dispensa senha; assim uma falha
    // do próprio comando não dispara um segundo pedido via pkexec.
    let sudo_ok = r.command_exists("sudo")
        && r.run(
            &Cmd::status("sudo", &["-n", "true"])
                .stdin(In::Null)
                .stdout(Out::Null)
                .stderr(Out::Null),
        )
        .map(|s| s.success())
        .unwrap_or(false);
    if sudo_ok {
        let st = run("sudo").context("Falha ao executar sudo")?;
        return match st.code {
            Some(0) => Ok(()),
            code => bail!("a operação com privilégios falhou (código {code:?})"),
        };
    }
    if !have_pkexec {
        bail!("elevação indisponível: instale pkexec (polkit) ou configure sudo");
    }
    let status = run("pkexec").context("Falha ao pedir permissão de administrador")?;
    match status.code {
        Some(0) => Ok(()),
        Some(126) | Some(127) => {
            bail!(
                "a permissão de administrador foi negada (código {:?})",
                status.code
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

fn command_text(r: &dyn CommandRunner, program: &str, args: &[&str]) -> String {
    r.run(&Cmd::output(program, args).stderr(Out::Null))
        .map(|o| o.stdout_text())
        .unwrap_or_default()
}

/// DNS em uso: resolv.conf + NetworkManager (`device show`) + systemd-resolved (`resolvectl`).
/// Ler só o resolv.conf não basta: com stub local ele mostra 127.0.0.53.
pub fn configured_dns() -> Vec<std::net::IpAddr> {
    configured_dns_with(&RealRunner)
}

pub(super) fn configured_dns_with(r: &dyn CommandRunner) -> Vec<std::net::IpAddr> {
    let mut all = Vec::new();
    let mut add = |text: String| {
        for ip in parse_dns_ips(&text) {
            if !all.contains(&ip) {
                all.push(ip);
            }
        }
    };
    let resolv = r.read_file("/etc/resolv.conf").unwrap_or_default();
    add(resolv
        .lines()
        .filter(|l| l.trim_start().starts_with("nameserver"))
        .collect::<Vec<_>>()
        .join("\n"));
    if r.command_exists("nmcli") {
        add(command_text(
            r,
            "nmcli",
            &["-t", "-f", "IP4.DNS,IP6.DNS", "device", "show"],
        ));
    }
    if r.command_exists("resolvectl") {
        add(command_text(r, "resolvectl", &["dns"]));
    }
    all
}

/// `true` se o DNS em uso ainda é o filtro do GuiaLar (resolv.conf ou perfil do NM).
pub fn cloudflare_dns_active() -> bool {
    cloudflare_dns_active_with(&RealRunner)
}

fn cloudflare_dns_active_with(r: &dyn CommandRunner) -> bool {
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
    if r.read_file("/etc/resolv.conf")
        .map(|t| cf(&t))
        .unwrap_or(false)
    {
        return true;
    }
    if r.command_exists("nmcli") {
        if let Ok(uuids) = get_active_nm_connections(r) {
            for uuid in uuids {
                if let Ok(b) = inspect_nm_connection_raw(r, &uuid) {
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
    revert_dns_fallback_with(&RealRunner)
}

fn revert_dns_fallback_with(r: &dyn CommandRunner) -> Result<()> {
    if !r.is_root() {
        return elevate_self(r, "internal-revert");
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    match detect_active_backend_with(r) {
        BackendKind::NetworkManager => {
            for uuid in get_active_nm_connections(r)? {
                for fam in ["ipv4", "ipv6"] {
                    let dns = format!("{fam}.dns");
                    let ign = format!("{fam}.ignore-auto-dns");
                    run_privileged_command(
                        r,
                        "nmcli",
                        &["connection", "modify", &uuid, &dns, "", &ign, "no"],
                    )?;
                }
                run_privileged_command(r, "nmcli", &["connection", "up", "uuid", &uuid])?;
            }
        }
        BackendKind::SystemdResolved => revert_systemd_resolved(r)?,
        BackendKind::OpenResolvRunit => revert_openresolv_runit(r)?,
        BackendKind::DirectResolvConf | BackendKind::WindowsPowerShell => {
            // Sem backup do DNS anterior: só tira as linhas do filtro do GuiaLar.
            let atual = r.read_file("/etc/resolv.conf").unwrap_or_default();
            let limpo: Vec<&str> = atual
                .lines()
                .filter(|l| !l.contains("Gerado pelo GuiaLar"))
                .filter(|l| {
                    !(l.trim_start().starts_with("nameserver")
                        && crate::config::line_has_filter_ip(l))
                })
                .collect();
            write_resolv_conf(r, &(limpo.join("\n") + "\n"))?;
        }
    }
    DnsStateManifest::remove_manifest()?;
    Ok(())
}

/// Troca o filtro com a proteção ligada, mantendo o DNS original do manifesto.
pub fn switch_mode(manifest: DnsStateManifest) -> Result<()> {
    switch_mode_with(&RealRunner, manifest)
}

pub(super) fn switch_mode_with(
    r: &dyn CommandRunner,
    mut manifest: DnsStateManifest,
) -> Result<()> {
    if !r.is_root() {
        return elevate_self(
            r,
            &format!("internal-apply --mode {}", current_mode().cli_name()),
        );
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
                    r,
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
                    r,
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
                run_privileged_command(r, "nmcli", &["connection", "up", "uuid", u])?;
            }
        }
        BackendKind::SystemdResolved => apply_systemd_resolved(r, &mut manifest.clone())?,
        BackendKind::OpenResolvRunit => apply_openresolv_runit(r, &mut manifest.clone())?,
        BackendKind::DirectResolvConf => {
            // `apply_direct_resolv_conf` regravaria o backup: usa uma cópia descartável.
            apply_direct_resolv_conf(r, &mut manifest.clone())?;
        }
        BackendKind::WindowsPowerShell => bail!("Backend Windows chamado em ambiente Linux"),
    }
    manifest.mode = current_mode();
    manifest.save_atomic()?;
    Ok(())
}

/// Aplica a proteção conforme o backend detectado e o modo escolhido.
pub fn apply_dns() -> Result<()> {
    apply_dns_with(&RealRunner)
}

pub(super) fn apply_dns_with(r: &dyn CommandRunner) -> Result<()> {
    if !r.is_root() {
        return elevate_self(
            r,
            &format!("internal-apply --mode {}", current_mode().cli_name()),
        );
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    let backend = detect_active_backend_with(r);
    let mut manifest = DnsStateManifest::new(backend.clone());

    match backend {
        BackendKind::NetworkManager => {
            apply_network_manager(r, &mut manifest)?;
        }
        BackendKind::SystemdResolved => {
            apply_systemd_resolved(r, &mut manifest)?;
        }
        BackendKind::OpenResolvRunit => {
            apply_openresolv_runit(r, &mut manifest)?;
        }
        BackendKind::DirectResolvConf => {
            apply_direct_resolv_conf(r, &mut manifest)?;
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
    revert_dns_with(&RealRunner, manifest)
}

pub(super) fn revert_dns_with(r: &dyn CommandRunner, manifest: &DnsStateManifest) -> Result<()> {
    if !r.is_root() {
        return elevate_self(r, "internal-revert");
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    match manifest.backend {
        BackendKind::NetworkManager => {
            revert_network_manager(r, manifest)?;
        }
        BackendKind::SystemdResolved => {
            revert_systemd_resolved(r)?;
        }
        BackendKind::OpenResolvRunit => {
            revert_openresolv_runit(r)?;
        }
        BackendKind::DirectResolvConf => {
            revert_direct_resolv_conf(r, manifest)?;
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

fn apply_network_manager(r: &dyn CommandRunner, manifest: &mut DnsStateManifest) -> Result<()> {
    let active_uuids = get_active_nm_connections(r)?;
    if active_uuids.is_empty() {
        bail!("Nenhuma conexão ativa do NetworkManager (ethernet/wifi) encontrada");
    }

    let ipv4_str = format!("{} {}", current_mode().ipv4()[0], current_mode().ipv4()[1]);
    let ipv6_str = format!("{} {}", current_mode().ipv6()[0], current_mode().ipv6()[1]);

    for uuid in &active_uuids {
        let backup = inspect_nm_connection(r, uuid)?;
        manifest.connections.push(backup);

        run_privileged_command(
            r,
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
            r,
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
        run_privileged_command(r, "nmcli", &["connection", "up", "uuid", uuid])?;
    }

    Ok(())
}

fn revert_network_manager(r: &dyn CommandRunner, manifest: &DnsStateManifest) -> Result<()> {
    for conn in &manifest.connections {
        let uuid = &conn.id;
        if conn.was_dhcp_ipv4 {
            run_privileged_command(
                r,
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
                r,
                "nmcli",
                &["connection", "modify", uuid, "ipv4.dns", &prev.join(" ")],
            )?;
        }

        if conn.was_dhcp_ipv6 {
            let _ = run_privileged_command(
                r,
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

        let _ = run_privileged_command(r, "nmcli", &["connection", "up", "uuid", uuid]);
    }
    Ok(())
}

fn get_active_nm_connections(r: &dyn CommandRunner) -> Result<Vec<String>> {
    let output = r
        .run(&Cmd::output(
            "nmcli",
            &["-t", "-f", "UUID,TYPE", "connection", "show", "--active"],
        ))
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

fn inspect_nm_connection_raw(r: &dyn CommandRunner, uuid: &str) -> Result<(Vec<Ipv4Addr>, bool)> {
    let b = inspect_nm_connection_unsanitized(r, uuid)?;
    Ok((b.original_ipv4_dns, b.was_dhcp_ipv4))
}

fn inspect_nm_connection(r: &dyn CommandRunner, uuid: &str) -> Result<ConnectionBackup> {
    let b = inspect_nm_connection_unsanitized(r, uuid)?;
    let (ips, dhcp) = crate::config::sanitize_original_dns(b.original_ipv4_dns, b.was_dhcp_ipv4);
    Ok(ConnectionBackup {
        original_ipv4_dns: ips,
        was_dhcp_ipv4: dhcp,
        ..b
    })
}

fn inspect_nm_connection_unsanitized(
    r: &dyn CommandRunner,
    uuid: &str,
) -> Result<ConnectionBackup> {
    let output = r
        .run(&Cmd::output(
            "nmcli",
            &[
                "-t",
                "-f",
                "ipv4.dns,ipv4.ignore-auto-dns",
                "connection",
                "show",
                uuid,
            ],
        ))
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

fn apply_systemd_resolved(r: &dyn CommandRunner, _manifest: &mut DnsStateManifest) -> Result<()> {
    if !r.path_exists("/etc/systemd/resolved.conf.d") {
        run_privileged_command(r, "mkdir", &["-p", "/etc/systemd/resolved.conf.d"])?;
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
        r,
        "/etc/systemd/resolved.conf.d/99-guialar.conf",
        &conf_content,
    )?;

    run_privileged_command(r, "systemctl", &["restart", "systemd-resolved"])?;
    Ok(())
}

fn revert_systemd_resolved(r: &dyn CommandRunner) -> Result<()> {
    let dropin_path = "/etc/systemd/resolved.conf.d/99-guialar.conf";
    if r.path_exists(dropin_path) {
        run_privileged_command(r, "rm", &["-f", dropin_path])?;
        let _ = run_privileged_command(r, "systemctl", &["restart", "systemd-resolved"]);
    }
    Ok(())
}

// ============================================================================
// 3. Runit & Openresolv Implementation (Void Linux / runit services)
// ============================================================================

fn apply_openresolv_runit(r: &dyn CommandRunner, _manifest: &mut DnsStateManifest) -> Result<()> {
    let conf = format!(
        "nameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
        current_mode().ipv4()[0],
        current_mode().ipv4()[1],
        current_mode().ipv6()[0],
        current_mode().ipv6()[1]
    );

    let status = r
        .run(&privileged(r, "resolvconf", &["-a", "guialar"]).stdin(In::Data(conf.into_bytes())))
        .context("Falha ao iniciar resolvconf")?;

    if !status.success() {
        bail!("Falha ao aplicar DNS via resolvconf no runit");
    }
    Ok(())
}

fn revert_openresolv_runit(r: &dyn CommandRunner) -> Result<()> {
    let status = r.run(&privileged(r, "resolvconf", &["-d", "guialar"]))?;

    if !status.success() {
        bail!("Falha ao remover interface guialar do resolvconf");
    }
    Ok(())
}

// ============================================================================
// 4. Direct /etc/resolv.conf Fallback (Atomic Replace)
// ============================================================================

fn apply_direct_resolv_conf(r: &dyn CommandRunner, manifest: &mut DnsStateManifest) -> Result<()> {
    if r.path_exists("/etc/resolv.conf") {
        let current = r.read_file("/etc/resolv.conf").unwrap_or_default();
        manifest.previous_resolv_conf = Some(current);
    }

    let content = format!(
        "# Gerado pelo GuiaLar Digital - Protecao Familiar\nnameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
        current_mode().ipv4()[0], current_mode().ipv4()[1], current_mode().ipv6()[0], current_mode().ipv6()[1]
    );

    write_resolv_conf(r, &content)
}

/// Grava `path` via `tee` (com privilégio) com o conteúdo em stdin, sem arquivo temporário.
fn write_privileged_file(r: &dyn CommandRunner, path: &str, content: &str) -> Result<()> {
    let out = r
        .run(
            &privileged(r, "tee", &[path])
                .stdin(In::Data(content.as_bytes().to_vec()))
                .stdout(Out::Null),
        )
        .with_context(|| format!("Falha ao iniciar tee para {path}"))?;
    if !out.success() {
        bail!("Falha ao gravar {path}");
    }
    Ok(())
}

/// Grava `/etc/resolv.conf` via `tee` com o conteúdo em stdin (sem arquivo temporário).
fn write_resolv_conf(r: &dyn CommandRunner, content: &str) -> Result<()> {
    write_privileged_file(r, "/etc/resolv.conf", content)
}

fn revert_direct_resolv_conf(r: &dyn CommandRunner, manifest: &DnsStateManifest) -> Result<()> {
    if let Some(ref prev) = manifest.previous_resolv_conf {
        write_resolv_conf(r, prev)?;
    }
    Ok(())
}

// ============================================================================
// Helpers de Sistema
// ============================================================================

fn is_service_active_runit(r: &dyn CommandRunner, service_name: &str) -> bool {
    if r.path_exists(&format!("/var/service/{service_name}")) {
        if let Ok(output) = r.run(&Cmd::output("sv", &["status", service_name])) {
            return output.stdout_text().starts_with("run:");
        }
    }
    false
}

/// Comando já prefixado com a elevação adequada (nenhuma se já é root).
fn privileged(r: &dyn CommandRunner, program: &str, args: &[&str]) -> Cmd {
    Cmd::status(program, args).prefixed(&r.privilege_prefix())
}

fn run_privileged_command(r: &dyn CommandRunner, program: &str, args: &[&str]) -> Result<()> {
    let status = r
        .run(&privileged(r, program, args))
        .with_context(|| format!("Falha ao executar {} com privilégios", program))?;
    if !status.success() {
        bail!("Comando {} finalizou com erro ({:?})", program, status.code);
    }
    Ok(())
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod runner_tests;

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

    #[test]
    fn entradas_vazias_ou_sem_ip() {
        assert!(parse_dns_ips("").is_empty());
        assert!(parse_dns_ips("   \n\t\n").is_empty());
        assert!(parse_dns_ips("options edns0 trust-ad\nsearch lan").is_empty());
        assert!(parse_dns_ips("Link 3 (wlan0):").is_empty());
        assert!(parse_dns_ips("nameserver 999.1.1.1\nnameserver nao-e-ip").is_empty());
    }

    #[test]
    fn ignora_comentarios_e_duplicados() {
        assert!(parse_dns_ips("; nameserver 8.8.8.8").is_empty());
        assert_eq!(
            parse_dns_ips("nameserver 1.1.1.3\n# x\nnameserver 1.1.1.3\nnameserver 1.0.0.3"),
            s(&["1.1.1.3", "1.0.0.3"])
        );
    }

    #[test]
    fn aceita_tab_virgula_sufixos_e_escopo_ipv6() {
        assert_eq!(parse_dns_ips("nameserver\t9.9.9.9"), s(&["9.9.9.9"]));
        assert_eq!(
            parse_dns_ips("Link 2 (eth0): 192.168.0.1, 8.8.8.8"),
            s(&["192.168.0.1", "8.8.8.8"])
        );
        assert_eq!(
            parse_dns_ips("nameserver fe80::1%eth0\nGlobal: 1.1.1.3#cloudflare-dns.com"),
            s(&["fe80::1", "1.1.1.3"])
        );
    }

    #[test]
    fn stub_local_tambem_e_devolvido() {
        // Quem decide o que conta é verify::check_configured_dns, não o parser.
        assert_eq!(
            parse_dns_ips("nameserver 127.0.0.53\noptions edns0"),
            s(&["127.0.0.53"])
        );
    }
}

//! Backend Linux para configuração e reversão de DNS.
//!
//! Suporta 4 estratégias em ordem de prioridade:
//! 1. NetworkManager (`nmcli` por UUID da conexão ativa)
//! 2. systemd-resolved (drop-in em `/etc/systemd/resolved.conf.d/99-guialar.conf`)
//! 3. Runit / Openresolv (`resolvconf` ou serviços em `/var/service`)
//! 4. Arquivo direto `/etc/resolv.conf` (com backup e substituição atômica)

use anyhow::{bail, Context, Result};
use std::net::{Ipv4Addr, Ipv6Addr};

use super::runner::{Cmd, CommandRunner, In, Out, RealRunner};
use super::transaction::{self, DnsBackend, PointError};
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

/// Cópia privada do binário, para elevar a cópia e não o AppImage (ADR-028, rev. 2).
///
/// O root não acessa o ponto de montagem FUSE do usuário, então dentro de um AppImage o
/// processo do usuário copia o próprio executável (`/proc/self/exe`, que ele lê) para uma
/// pasta nova e aleatória, `0700`, e eleva essa cópia. Não há shell, extração como root nem
/// `/tmp`; o polkit mostra o caminho do GuiaLar. A pasta é apagada ao sair (`Drop`), com
/// sucesso ou com erro.
struct StagedBinary {
    dir: std::path::PathBuf,
    exe: std::path::PathBuf,
}

impl Drop for StagedBinary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A pasta está num sistema de arquivos montado com `noexec`? Lê `/proc/self/mountinfo` e
/// pega a montagem de prefixo mais longo. Sem poder ler, assume que não (não bloqueia).
fn is_noexec(path: &std::path::Path) -> bool {
    let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    let real = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    noexec_in_mountinfo(&text, &real)
}

fn noexec_in_mountinfo(mountinfo: &str, path: &std::path::Path) -> bool {
    let unescape = |s: &str| s.replace("\\040", " ");
    mountinfo
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let mount = std::path::PathBuf::from(unescape(f.get(4)?));
            let opts = *f.get(5)?;
            path.starts_with(&mount)
                .then_some((mount, opts.split(',').any(|o| o == "noexec")))
        })
        .max_by_key(|(m, _)| m.components().count())
        .is_some_and(|(_, noexec)| noexec)
}

/// Primeira base executável entre as candidatas; erro claro se todas forem `noexec`.
fn pick_base(
    candidates: Vec<std::path::PathBuf>,
    noexec: impl Fn(&std::path::Path) -> bool,
) -> Result<std::path::PathBuf> {
    let mut recusadas = Vec::new();
    for c in candidates {
        if noexec(&c) {
            recusadas.push(c.display().to_string());
        } else {
            return Ok(c);
        }
    }
    bail!(
        "não há pasta privada onde o programa possa ser executado com permissão de administrador (montada com noexec: {}). Nada foi alterado.",
        recusadas.join(", ")
    )
}

/// Pastas privadas do usuário onde a cópia pode viver, em ordem: `$XDG_RUNTIME_DIR` (se for
/// `0700` do próprio usuário) e `~/.cache/guialar` (também `0700`). Nunca `/tmp`.
fn staging_candidates() -> Result<Vec<std::path::PathBuf>> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = crate::sysutil::current_ids().map(|i| i.0);
    let private = |p: &std::path::Path| {
        std::fs::symlink_metadata(p)
            .is_ok_and(|m| m.file_type().is_dir() && Some(m.uid()) == uid && m.mode() & 0o077 == 0)
    };
    let mut out = Vec::new();
    if let Some(x) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from) {
        if x.is_absolute() && private(&x) {
            out.push(x);
        }
    }
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache")));
    if let Some(cache) = cache {
        let base = cache.join("guialar");
        if std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&base)
            .is_ok()
            && private(&base)
        {
            out.push(base);
        }
    }
    if out.is_empty() {
        bail!("sem $XDG_RUNTIME_DIR nem $HOME para guardar a cópia do programa");
    }
    Ok(out)
}

/// Apaga as cópias `guialar-<12 hex>` com mais de 1 h que um `SIGKILL` ou uma queda deixou.
fn clean_stale_copies(base: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(base) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(tag) = name.strip_prefix("guialar-") else {
            continue;
        };
        if tag.len() != 12 || !tag.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(m) = std::fs::symlink_metadata(e.path()) else {
            continue;
        };
        let velha = m
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d.as_secs() > 3600);
        if m.file_type().is_dir() && velha {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

fn stage_binary() -> Result<StagedBinary> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let base = pick_base(staging_candidates()?, is_noexec)?;
    clean_stale_copies(&base);
    let mut last = None;
    for _ in 0..8 {
        let mut tag = [0u8; 6];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut tag))
            .context("não foi possível ler /dev/urandom")?;
        let name: String = tag.iter().map(|b| format!("{b:02x}")).collect();
        let dir = base.join(format!("guialar-{name}"));
        match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            Ok(()) => {
                let exe = dir.join("guialar");
                let staged = StagedBinary { dir, exe };
                let mut src = std::fs::File::open("/proc/self/exe")
                    .context("não foi possível ler o programa")?;
                let mut dst = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o700)
                    .open(&staged.exe)
                    .context("não foi possível criar a cópia do programa")?;
                std::io::copy(&mut src, &mut dst).context("não foi possível copiar o programa")?;
                return Ok(staged);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = Some(e),
            Err(e) => return Err(e).context("não foi possível criar a pasta privada"),
        }
    }
    Err(last
        .map(Into::into)
        .unwrap_or_else(|| anyhow::anyhow!("sem pasta privada")))
}

/// Reexecuta este programa elevado **uma única vez** (um só pedido de senha) com
/// `internal-apply` / `internal-revert`. Dentro de um AppImage eleva uma cópia do binário
/// (o ponto de montagem FUSE não é acessível ao root).
fn elevate_self(r: &dyn CommandRunner, subcommand: &str) -> Result<()> {
    crate::sysutil::OpLock::check_free()?;
    // `APPIMAGE` só vale se este executável está dentro do `APPDIR` (ADR-028, rev. 2)
    let in_appimage = crate::sysutil::running_appimage().is_some();
    let staged = if in_appimage {
        Some(stage_binary()?)
    } else {
        None
    };
    let exe = match &staged {
        Some(s) => s.exe.clone(),
        None => std::env::current_exe().context("não foi possível localizar o executável")?,
    };
    let args_for = |tool: &str| -> Vec<std::ffi::OsString> {
        let mut v: Vec<std::ffi::OsString> = Vec::new();
        if tool == "sudo" {
            v.push("-n".into()); // nunca pede senha sem TTY
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
            Some(code @ (126 | 127)) if in_appimage => bail!(
                "o AppImage não pôde ser executado com privilégios de administrador (código {code}). Nada foi alterado."
            ),
            code => bail!("a operação com privilégios falhou (código {code:?})"),
        };
    }
    if !have_pkexec {
        bail!("elevação indisponível: instale pkexec (polkit) ou configure sudo");
    }
    let status = run("pkexec").context("Falha ao pedir permissão de administrador")?;
    match status.code {
        Some(0) => Ok(()),
        // O pkexec sai com 126 se a janela é fechada e com 127 em «Not authorized» (senha
        // errada, política): nunca quer dizer «não executou». O código real vai na mensagem.
        Some(code @ (126 | 127)) => bail!(
            "a permissão de administrador foi negada ou a senha não foi aceita (código {code})"
        ),
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

/// UUIDs de todas as conexões do NetworkManager agora (para validar o manifesto antigo).
fn known_connection_ids_with(r: &dyn CommandRunner) -> Vec<String> {
    if !r.command_exists("nmcli") {
        return Vec::new();
    }
    r.run(&Cmd::output(
        "nmcli",
        &["-t", "-f", "UUID", "connection", "show"],
    ))
    .ok()
    .map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// Migração do manifesto antigo da beta (ADR-031); ver `platform::migrate_legacy`.
pub fn migrate_legacy() -> Result<Option<String>> {
    migrate_legacy_with(&RealRunner)
}

pub(super) fn migrate_legacy_with(r: &dyn CommandRunner) -> Result<Option<String>> {
    super::migrate_legacy_generic(
        detect_active_backend_with(r),
        &known_connection_ids_with(r),
        |m| revert_dns_with(r, m),
        || revert_dns_fallback_with(r),
    )
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
    let backend = backend_for(&detect_active_backend_with(r), r)?;
    transaction::revert_fallback(&*backend, || Ok(()))
}

/// Troca o filtro com a proteção ligada, mantendo o DNS original do manifesto.
pub fn switch_mode(manifest: DnsStateManifest) -> Result<()> {
    switch_mode_with(&RealRunner, manifest)
}

pub(super) fn switch_mode_with(r: &dyn CommandRunner, manifest: DnsStateManifest) -> Result<()> {
    if !r.is_root() {
        return elevate_self(
            r,
            &format!("internal-apply --mode {}", current_mode().cli_name()),
        );
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    let backend = backend_for(&manifest.backend, r)?;
    transaction::switch(&*backend, manifest, current_mode(), |_| Ok(()))
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
    let backend = backend_for(&detect_active_backend_with(r), r)?;
    transaction::apply(&*backend, current_mode())
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
    let backend = backend_for(&manifest.backend, r)?;
    transaction::revert(&*backend, manifest, || Ok(()))
}

/// O adaptador do subsistema de DNS `kind` (a costura de `transaction`).
fn backend_for<'a>(
    kind: &BackendKind,
    r: &'a dyn CommandRunner,
) -> Result<Box<dyn DnsBackend + 'a>> {
    Ok(match kind {
        BackendKind::NetworkManager => Box::new(NmBackend(r)),
        BackendKind::SystemdResolved => Box::new(ResolvedBackend(r)),
        BackendKind::OpenResolvRunit => Box::new(ResolvconfBackend(r)),
        BackendKind::DirectResolvConf => Box::new(DirectBackend(r)),
        BackendKind::WindowsPowerShell => bail!("Backend Windows chamado em ambiente Linux"),
    })
}

// ============================================================================
// 1. NetworkManager Implementation (via nmcli)
// ============================================================================

struct NmBackend<'a>(&'a dyn CommandRunner);

impl DnsBackend for NmBackend<'_> {
    fn kind(&self) -> BackendKind {
        BackendKind::NetworkManager
    }

    /// Lê o estado de TODAS as conexões ativas antes de alterar qualquer uma.
    fn inspect(&self, m: &mut DnsStateManifest) -> Result<()> {
        let uuids = get_active_nm_connections(self.0)?;
        if uuids.is_empty() {
            bail!("Nenhuma conexão ativa do NetworkManager (ethernet/wifi) encontrada");
        }
        for uuid in &uuids {
            m.connections.push(inspect_nm_connection(self.0, uuid)?);
        }
        Ok(())
    }

    fn point_to(
        &self,
        m: &DnsStateManifest,
        mode: crate::config::FilterMode,
    ) -> Result<(), PointError> {
        let r = self.0;
        let v4 = format!("{} {}", mode.ipv4()[0], mode.ipv4()[1]);
        let v6 = format!("{} {}", mode.ipv6()[0], mode.ipv6()[1]);
        let mut tocadas: Vec<String> = Vec::new();
        for conn in &m.connections {
            let u = &conn.id;
            // a conexão entra na lista antes do primeiro `modify`: se falhar pela metade, o
            // desfazer a inclui
            tocadas.push(u.clone());
            let r1 = run_privileged_command(
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
            );
            if let Err(e) = r1 {
                return Err(PointError::partial(e, tocadas));
            }
            // IPv6 é melhor esforço: uma rede sem IPv6 não impede o filtro IPv4
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
            if let Err(e) = run_privileged_command(r, "nmcli", &["connection", "up", "uuid", u]) {
                return Err(PointError::partial(e, tocadas));
            }
        }
        Ok(())
    }

    fn restore(&self, m: &DnsStateManifest) -> Result<()> {
        revert_network_manager(self.0, m)
    }

    /// Tenta TODAS as conexões ativas e junta as falhas: um erro numa não deixa as outras no
    /// filtro.
    fn reset(&self) -> Result<()> {
        let r = self.0;
        let mut falhas: Vec<String> = Vec::new();
        for uuid in get_active_nm_connections(r)? {
            let feito = (|| -> Result<()> {
                for fam in ["ipv4", "ipv6"] {
                    let dns = format!("{fam}.dns");
                    let ign = format!("{fam}.ignore-auto-dns");
                    run_privileged_command(
                        r,
                        "nmcli",
                        &["connection", "modify", &uuid, &dns, "", &ign, "no"],
                    )?;
                }
                run_privileged_command(r, "nmcli", &["connection", "up", "uuid", &uuid])
            })();
            if let Err(e) = feito {
                falhas.push(format!("{uuid}: {e:#}"));
            }
        }
        if !falhas.is_empty() {
            bail!(
                "Falha ao devolver o DNS automático em {} conexão(ões): {}",
                falhas.len(),
                falhas.join(" | ")
            );
        }
        Ok(())
    }
}

/// Devolve cada conexão ao que o manifesto guardou. Tenta TODAS e junta as falhas: um erro
/// numa conexão não deixa as outras presas no filtro.
fn revert_network_manager(r: &dyn CommandRunner, manifest: &DnsStateManifest) -> Result<()> {
    let falhas: Vec<String> = manifest
        .connections
        .iter()
        .filter_map(|conn| {
            restore_nm_connection(r, conn)
                .err()
                .map(|e| format!("{}: {e:#}", conn.id))
        })
        .collect();
    if !falhas.is_empty() {
        bail!(
            "Falha ao reverter o DNS em {} conexão(ões): {}",
            falhas.len(),
            falhas.join(" | ")
        );
    }
    Ok(())
}

fn restore_nm_connection(r: &dyn CommandRunner, conn: &ConnectionBackup) -> Result<()> {
    let uuid = &conn.id;
    // `was_dhcp_*` = a conexão aceitava o DNS da rede (`ignore-auto-dns=no`). Ela pode ter
    // IPs fixos somados ao do DHCP: restaura os fixos (lista vazia => `""`, como sempre).
    if conn.was_dhcp_ipv4 {
        let prev = join_ips(&conn.original_ipv4_dns);
        run_privileged_command(
            r,
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv4.dns",
                &prev,
                "ipv4.ignore-auto-dns",
                "no",
            ],
        )?;
    } else if conn.original_ipv4_dns.is_empty() {
        // `ignore-auto-dns=yes` sem IP fixo era do usuário: tira o filtro e mantém o `yes`
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
                "yes",
            ],
        )?;
    } else {
        run_privileged_command(
            r,
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv4.dns",
                &join_ips(&conn.original_ipv4_dns),
            ],
        )?;
    }

    if conn.was_dhcp_ipv6 {
        let prev = join_ips(&conn.original_ipv6_dns);
        let _ = run_privileged_command(
            r,
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv6.dns",
                &prev,
                "ipv6.ignore-auto-dns",
                "no",
            ],
        );
    } else if conn.original_ipv6_dns.is_empty() {
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
                "yes",
            ],
        );
    } else {
        let _ = run_privileged_command(
            r,
            "nmcli",
            &[
                "connection",
                "modify",
                uuid,
                "ipv6.dns",
                &join_ips(&conn.original_ipv6_dns),
            ],
        );
    }

    let _ = run_privileged_command(r, "nmcli", &["connection", "up", "uuid", uuid]);
    Ok(())
}

/// IPs separados por espaço, como o `nmcli connection modify` aceita.
fn join_ips<T: std::fmt::Display>(ips: &[T]) -> String {
    ips.iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>()
        .join(" ")
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

fn inspect_nm_connection(r: &dyn CommandRunner, uuid: &str) -> Result<ConnectionBackup> {
    let b = inspect_nm_connection_unsanitized(r, uuid)?;
    let (ips, dhcp) = crate::config::sanitize_original_dns(b.original_ipv4_dns, b.was_dhcp_ipv4);
    // IPv6 numa chamada à parte (o argv do IPv4 acima não muda). Se falhar, cai no padrão
    // antigo («DHCP, sem fixos») em vez de impedir o apply.
    let (ips6, dhcp6) = inspect_nm_ipv6(r, uuid).unwrap_or_else(|_| {
        eprintln!("{}", aviso_leitura_ipv6(uuid));
        (Vec::new(), true)
    });
    let (ips6, dhcp6) = crate::config::sanitize_original_dns6(ips6, dhcp6);
    Ok(ConnectionBackup {
        original_ipv4_dns: ips,
        was_dhcp_ipv4: dhcp,
        original_ipv6_dns: ips6,
        was_dhcp_ipv6: dhcp6,
        ..b
    })
}

/// Aviso quando o IPv6 da conexão não pôde ser lido: o apply segue e o revert o devolve ao
/// automático (só o id da conexão, nenhum dado de rede).
fn aviso_leitura_ipv6(uuid: &str) -> String {
    format!("⚠️ Não consegui ler o DNS IPv6 de {uuid}; ao desativar, ele volta ao automático.")
}

/// `ipv6.dns` e `ipv6.ignore-auto-dns` da conexão: (IPs fixos, aceita o DNS da rede).
fn inspect_nm_ipv6(r: &dyn CommandRunner, uuid: &str) -> Result<(Vec<Ipv6Addr>, bool)> {
    let output = r
        .run(&Cmd::output(
            "nmcli",
            &[
                "-t",
                "-f",
                "ipv6.dns,ipv6.ignore-auto-dns",
                "connection",
                "show",
                uuid,
            ],
        ))
        .context("Falha ao inspecionar o IPv6 da conexão do NetworkManager")?;
    if !output.success() {
        bail!("nmcli saiu com erro ao ler o IPv6 da conexão");
    }
    Ok(parse_nm_ipv6(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_nm_ipv6(stdout: &str) -> (Vec<Ipv6Addr>, bool) {
    let mut was_dhcp = true;
    let mut ips = Vec::new();
    for line in stdout.lines() {
        if let Some(v) = line.strip_prefix("ipv6.ignore-auto-dns:") {
            if v.contains("yes") {
                was_dhcp = false;
            }
        } else if let Some(v) = line.strip_prefix("ipv6.dns:") {
            // `-t` escapa `:` e `\` quando há vários campos; desfaz por garantia.
            let v = v.replace("\\:", ":");
            for tok in v.split(|c: char| c.is_whitespace() || c == ',') {
                if let Ok(ip) = tok.parse::<Ipv6Addr>() {
                    ips.push(ip);
                }
            }
        }
    }
    (ips, was_dhcp)
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

struct ResolvedBackend<'a>(&'a dyn CommandRunner);

impl DnsBackend for ResolvedBackend<'_> {
    fn kind(&self) -> BackendKind {
        BackendKind::SystemdResolved
    }
    fn inspect(&self, _m: &mut DnsStateManifest) -> Result<()> {
        Ok(()) // o drop-in é só nosso: não há DNS anterior a guardar
    }
    fn point_to(
        &self,
        _m: &DnsStateManifest,
        mode: crate::config::FilterMode,
    ) -> Result<(), PointError> {
        apply_systemd_resolved(self.0, mode).map_err(Into::into)
    }
    fn restore(&self, _m: &DnsStateManifest) -> Result<()> {
        revert_systemd_resolved(self.0)
    }
    fn reset(&self) -> Result<()> {
        revert_systemd_resolved(self.0)
    }
}

fn apply_systemd_resolved(r: &dyn CommandRunner, mode: crate::config::FilterMode) -> Result<()> {
    if !r.path_exists("/etc/systemd/resolved.conf.d") {
        run_privileged_command(r, "mkdir", &["-p", "/etc/systemd/resolved.conf.d"])?;
    }

    let conf_content = format!(
        "[Resolve]\nDNS={} {} {} {}\n",
        mode.ipv4()[0],
        mode.ipv4()[1],
        mode.ipv6()[0],
        mode.ipv6()[1]
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

/// Inócuo se o drop-in não existe (serve para desfazer um `apply` que falhou no meio).
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

struct ResolvconfBackend<'a>(&'a dyn CommandRunner);

impl DnsBackend for ResolvconfBackend<'_> {
    fn kind(&self) -> BackendKind {
        BackendKind::OpenResolvRunit
    }
    fn inspect(&self, _m: &mut DnsStateManifest) -> Result<()> {
        Ok(()) // a interface «guialar» é só nossa: não há DNS anterior a guardar
    }
    fn point_to(
        &self,
        _m: &DnsStateManifest,
        mode: crate::config::FilterMode,
    ) -> Result<(), PointError> {
        apply_openresolv_runit(self.0, mode).map_err(Into::into)
    }
    fn restore(&self, _m: &DnsStateManifest) -> Result<()> {
        revert_openresolv_runit(self.0)
    }
    fn reset(&self) -> Result<()> {
        revert_openresolv_runit(self.0)
    }
}

fn apply_openresolv_runit(r: &dyn CommandRunner, mode: crate::config::FilterMode) -> Result<()> {
    let conf = format!(
        "nameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
        mode.ipv4()[0],
        mode.ipv4()[1],
        mode.ipv6()[0],
        mode.ipv6()[1]
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

struct DirectBackend<'a>(&'a dyn CommandRunner);

impl DnsBackend for DirectBackend<'_> {
    fn kind(&self) -> BackendKind {
        BackendKind::DirectResolvConf
    }

    fn inspect(&self, m: &mut DnsStateManifest) -> Result<()> {
        if self.0.path_exists("/etc/resolv.conf") {
            // se não lê, não há backup: um erro, não um arquivo vazio que o desfazer gravaria
            let atual = self
                .0
                .read_file("/etc/resolv.conf")
                .context("Não consegui ler /etc/resolv.conf; nada foi alterado")?;
            m.previous_resolv_conf = Some(atual);
        }
        Ok(())
    }

    fn point_to(
        &self,
        _m: &DnsStateManifest,
        mode: crate::config::FilterMode,
    ) -> Result<(), PointError> {
        let content = format!(
            "# Gerado pelo GuiaLar Digital - Protecao Familiar\nnameserver {}\nnameserver {}\nnameserver {}\nnameserver {}\n",
            mode.ipv4()[0],
            mode.ipv4()[1],
            mode.ipv6()[0],
            mode.ipv6()[1]
        );
        write_resolv_conf(self.0, &content).map_err(Into::into)
    }

    /// Só regrava se o arquivo mudou: assim desfazer um `apply` que falhou antes de gravar
    /// nada não precisa de privilégio nem escreve à toa.
    fn restore(&self, m: &DnsStateManifest) -> Result<()> {
        match &m.previous_resolv_conf {
            Some(prev) if self.0.read_file("/etc/resolv.conf").as_ref() != Some(prev) => {
                write_resolv_conf(self.0, prev)
            }
            _ => Ok(()),
        }
    }

    /// Sem backup do DNS anterior: só tira as linhas do filtro do GuiaLar.
    fn reset(&self) -> Result<()> {
        let atual = self.0.read_file("/etc/resolv.conf").unwrap_or_default();
        let limpo: Vec<&str> = atual
            .lines()
            .filter(|l| !l.contains("Gerado pelo GuiaLar"))
            .filter(|l| {
                !(l.trim_start().starts_with("nameserver") && crate::config::line_has_filter_ip(l))
            })
            .collect();
        write_resolv_conf(self.0, &(limpo.join("\n") + "\n"))
    }
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

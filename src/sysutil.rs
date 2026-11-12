//! Utilitários de sistema compartilhados: elevação, home efetivo e PATH.
#![cfg_attr(not(unix), allow(dead_code))]
#![cfg_attr(unix, allow(dead_code))]

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Executável existe no `PATH` (sem criar processo `which`).
pub fn command_exists(command: &str) -> bool {
    if command.is_empty() {
        return false;
    }
    let candidate = Path::new(command);
    if candidate.components().count() > 1 {
        return candidate.is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        !dir.as_os_str().is_empty()
            && (dir.join(command).is_file() || dir.join(format!("{command}.exe")).is_file())
    })
}

/// UID efetivo lido de `/proc/self/status` (sem dependência de libc).
pub fn effective_uid(status_text: &str) -> Option<u32> {
    status_text
        .lines()
        .find(|l| l.starts_with("Uid:"))?
        .split_whitespace()
        .nth(2)?
        .parse()
        .ok()
}

pub fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| effective_uid(&s))
        == Some(0)
}

/// UID e GID efetivos deste processo (de `/proc/self/status`).
#[cfg(unix)]
pub fn current_ids() -> Option<(u32, u32)> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let col = |key: &str| -> Option<u32> {
        s.lines()
            .find(|l| l.starts_with(key))?
            .split_whitespace()
            .nth(2)?
            .parse()
            .ok()
    };
    Some((col("Uid:")?, col("Gid:")?))
}

/// O AppImage em que este processo roda, ou `None` (ADR-028, rev. 2).
///
/// `APPIMAGE` é herdado por todo processo filho de outro AppImage (um terminal de editor, por
/// exemplo), então só vale quando o executável está **dentro** do `APPDIR` que o runtime
/// exporta junto. Puro: recebe o ambiente.
pub fn running_appimage_from(
    appimage: Option<&std::ffi::OsStr>,
    appdir: Option<&std::ffi::OsStr>,
    exe: &Path,
) -> Option<PathBuf> {
    let appimage = appimage.filter(|v| !v.is_empty())?;
    let appdir = appdir.filter(|v| !v.is_empty())?;
    let appdir = Path::new(appdir);
    if !appdir.is_absolute() {
        return None;
    }
    let inside = exe.starts_with(appdir)
        || match (exe.canonicalize(), appdir.canonicalize()) {
            (Ok(e), Ok(d)) => e.starts_with(d),
            _ => false,
        };
    inside.then(|| PathBuf::from(appimage))
}

/// [`running_appimage_from`] com o ambiente e o executável deste processo.
pub fn running_appimage() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    running_appimage_from(
        std::env::var_os("APPIMAGE").as_deref(),
        std::env::var_os("APPDIR").as_deref(),
        &exe,
    )
}

/// Prefixo de elevação: vazio se já é root; `pkexec` com sessão gráfica; senão `sudo`.
pub fn privilege_prefix(
    root: bool,
    graphical: bool,
    has_pkexec: bool,
    has_sudo: bool,
) -> Vec<&'static str> {
    if root {
        Vec::new()
    } else if graphical && has_pkexec {
        vec!["pkexec"]
    } else if has_sudo {
        vec!["sudo"]
    } else {
        vec!["pkexec"]
    }
}

pub fn current_privilege_prefix() -> Vec<&'static str> {
    let graphical =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    privilege_prefix(
        is_root(),
        graphical,
        command_exists("pkexec"),
        command_exists("sudo"),
    )
}

/// Home do usuário em `passwd_text` (formato `/etc/passwd`).
pub fn home_from_passwd(passwd_text: &str, user: &str) -> Option<PathBuf> {
    passwd_text
        .lines()
        .map(|l| l.split(':').collect::<Vec<_>>())
        .find(|f| f.len() >= 6 && f[0] == user)
        .map(|f| PathBuf::from(f[5]))
}

/// Entrada de `/etc/passwd`: (usuário, uid, gid, home).
pub type PasswdEntry = (String, u32, u32, PathBuf);

fn parse_passwd_line(l: &str) -> Option<PasswdEntry> {
    let f: Vec<&str> = l.split(':').collect();
    if f.len() < 6 {
        return None;
    }
    Some((
        f[0].to_string(),
        f[2].parse().ok()?,
        f[3].parse().ok()?,
        PathBuf::from(f[5]),
    ))
}

pub fn passwd_by_name(passwd: &str, user: &str) -> Option<PasswdEntry> {
    passwd
        .lines()
        .filter_map(parse_passwd_line)
        .find(|e| e.0 == user)
}

pub fn passwd_by_uid(passwd: &str, uid: u32) -> Option<PasswdEntry> {
    passwd
        .lines()
        .filter_map(parse_passwd_line)
        .find(|e| e.1 == uid)
}

/// Usuário real por trás de `sudo`/`pkexec` (nunca root). Puro: recebe o ambiente.
pub fn caller_from_env(
    sudo_uid: Option<&str>,
    sudo_gid: Option<&str>,
    sudo_user: Option<&str>,
    pkexec_uid: Option<&str>,
    passwd: &str,
) -> Option<PasswdEntry> {
    let entry = if let Some(uid) = pkexec_uid.and_then(|v| v.parse::<u32>().ok()) {
        passwd_by_uid(passwd, uid)
    } else if let Some(uid) = sudo_uid.and_then(|v| v.parse::<u32>().ok()) {
        let mut e = passwd_by_uid(passwd, uid)?;
        if let Some(g) = sudo_gid.and_then(|v| v.parse::<u32>().ok()) {
            e.2 = g;
        }
        Some(e)
    } else {
        sudo_user.and_then(|u| passwd_by_name(passwd, u))
    }?;
    (entry.1 != 0).then_some(entry)
}

/// Usuário que chamou `sudo`/`pkexec`, lido do ambiente do processo (só quando root).
pub fn caller() -> Option<PasswdEntry> {
    if !is_root() {
        return None;
    }
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    let get = |k: &str| std::env::var(k).ok();
    caller_from_env(
        get("SUDO_UID").as_deref(),
        get("SUDO_GID").as_deref(),
        get("SUDO_USER").as_deref(),
        get("PKEXEC_UID").as_deref(),
        &passwd,
    )
}

/// Home efetivo para o manifesto: sob `sudo`/`pkexec`, o do usuário que chamou (não `/root`).
pub fn effective_home() -> Option<PathBuf> {
    if let Some(c) = caller() {
        return Some(c.3);
    }
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Devolve a posse de `path` ao usuário real quando rodando como root via sudo/pkexec.
#[cfg(unix)]
pub fn give_to_caller(path: &Path) {
    if let Some((_, uid, gid, _)) = caller() {
        let _ = std::os::unix::fs::chown(path, Some(uid), Some(gid));
    }
}

/// Codifica um script para `powershell -EncodedCommand` (UTF-16LE em base64).
///
/// Evita qualquer perda de aspas na linha de comando do Windows (causa do erro
/// `'ALIAS=' is not recognized` visto na versão Java).
pub fn encode_powershell(script: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        out.push(T[(b[0] >> 2) as usize] as char);
        out.push(T[(((b[0] & 3) << 4) | (b[1] >> 4)) as usize] as char);
        out.push(if c.len() > 1 {
            T[(((b[1] & 15) << 2) | (b[2] >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[(b[2] & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// Alias de interface seguro para interpolar em string PowerShell de aspas simples.
pub fn is_safe_ps_alias(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '#'))
}

/// Trava de operação: impede dois `apply`/`revert` ao mesmo tempo (o NetworkManager
/// quebra com ativações paralelas). Trava velha (>120 s) é considerada abandonada.
/// `acquire()` é reentrante no mesmo processo (o subcomando interno já segura a trava).
pub struct OpLock(Option<PathBuf>);

static HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const LOCK_BUSY: &str =
    "Outra operação do GuiaLar já está em andamento. Aguarde terminar e tente de novo.";

fn lock_path() -> PathBuf {
    #[cfg(test)]
    crate::testenv::assert_sandboxed("guialar-op.lock");
    lock_path_raw()
}

/// Pasta da trava no Linux: só o root grava ali (`/run` é `root 0755`), o usuário só lê.
/// Nunca `temp_dir()`: com `TMPDIR` diferente o processo elevado veria outra trava, e `/tmp`
/// é compartilhado e previsível (ADR-028, rev. 2).
#[cfg(target_os = "linux")]
const LOCK_DIR: &str = "/run/guialar";

/// Sem a guarda de teste: só calcula/compara o caminho (usado no `Drop`).
fn lock_path_raw() -> PathBuf {
    #[cfg(test)]
    if let Some(d) = crate::testenv::lock_dir() {
        return d.join("guialar-op.lock");
    }
    #[cfg(target_os = "linux")]
    {
        Path::new(LOCK_DIR).join("op.lock")
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Windows: na pasta de estado do sistema (`%ProgramData%\GuiaLar`, ADR-031), a mesma
        // para a conta padrão e para o administrador que digita a senha.
        crate::statedir::state_dir()
            .map(|d| d.join("op.lock"))
            .unwrap_or_else(|_| std::env::temp_dir().join("guialar-op.lock"))
    }
}

/// Garante `/run/guialar` (root, `0755`, pasta de verdade) antes de criar a trava. Só para o
/// caminho de produção; nos testes a trava fica no `Sandbox`.
#[cfg(target_os = "linux")]
fn ensure_lock_dir(lock: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let Some(dir) = lock.parent().filter(|d| *d == Path::new(LOCK_DIR)) else {
        return Ok(());
    };
    match std::fs::symlink_metadata(dir) {
        Ok(m) => {
            if !m.file_type().is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
                bail!(
                    "{} não é uma pasta segura do root; recuso criar a trava",
                    dir.display()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o755)
                .create(dir)
                .with_context(|| format!("não foi possível criar {}", dir.display()))?;
            // com `umask 077` a pasta nasceria 0700 e o usuário não leria o PID da trava
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755))
                .with_context(|| format!("permissões de {}", dir.display()))?;
        }
        Err(e) => return Err(e).context("não foi possível checar a pasta da trava"),
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn ensure_lock_dir(lock: &Path) -> Result<()> {
    // Windows: cria (ou refaz) a pasta de estado com a ACL protegida antes da trava.
    match (lock.parent(), crate::statedir::state_dir()) {
        (Some(p), Ok(dir)) if p == dir => crate::statedir::ensure_dir().map(|_| ()),
        _ => Ok(()),
    }
}

#[cfg(test)]
pub fn lock_path_for_test() -> PathBuf {
    lock_path()
}

/// Trava abandonada: velha (>120 s), vazia/sem PID válido (ex.: `touch`) ou com dono morto
/// (`kill -9`). Um arquivo recém-criado (<50 ms) ainda pode estar prestes a receber o PID.
fn lock_is_stale(path: &Path) -> bool {
    let age = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok());
    let Some(age) = age else { return false };
    if age.as_secs() > 120 {
        return true;
    }
    if age.as_millis() < 50 {
        return false;
    }
    match std::fs::read_to_string(path)
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
    {
        None => true, // sem PID válido
        Some(pid) => !process_alive(pid),
    }
}

/// O processo existe? Só sabemos responder no Linux (`/proc`); fora dele assume que sim.
fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        true
    }
}

/// Cria a trava sem seguir link (`O_CREAT|O_EXCL|O_NOFOLLOW`) e legível por todos (`0644`):
/// quem não é root só a lê, para saber se há operação em andamento.
fn create_lock_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o644).custom_flags(crate::safefs::O_NOFOLLOW);
    }
    let f = o.open(path)?;
    #[cfg(target_os = "linux")]
    {
        // a `umask` do root (077) esconderia o PID do usuário: fixa 0644 de verdade
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(f)
}

impl OpLock {
    pub fn acquire() -> Result<Self> {
        if HELD.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(Self(None)); // já seguro por este processo
        }
        let path = lock_path();
        ensure_lock_dir(&path)?;
        let l = Self::acquire_in(&path)?;
        HELD.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(l)
    }

    /// Falha se OUTRO processo está operando (para quem não é root/admin e só vai elevar).
    pub fn check_free() -> Result<()> {
        let path = lock_path();
        if HELD.load(std::sync::atomic::Ordering::SeqCst) || !path.exists() || lock_is_stale(&path)
        {
            return Ok(());
        }
        // Só respeita a trava se ela é do sistema (dono confiável): uma trava plantada por
        // uma conta comum não pode bloquear o apply e o revert para sempre (ADR-031).
        if !crate::statedir::is_trusted(&path) {
            eprintln!(
                "⚠️ Ignorando a trava {}: o dono não é o do sistema.",
                path.display()
            );
            return Ok(());
        }
        bail!(LOCK_BUSY)
    }

    pub fn acquire_in(path: &Path) -> Result<Self> {
        for _ in 0..2 {
            match create_lock_file(path) {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = write!(f, "{}", std::process::id());
                    let _ = f.flush();
                    return Ok(Self(Some(path.to_path_buf())));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // um link no lugar da trava (só o root grava em `/run/guialar`): não o
                    // seguimos nem o apagamos
                    if std::fs::symlink_metadata(path)
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false)
                    {
                        bail!("a trava de operação é um link; recuso seguir");
                    }
                    if lock_is_stale(path) {
                        let _ = std::fs::remove_file(path);
                        continue;
                    }
                    bail!(LOCK_BUSY);
                }
                Err(e) => return Err(e).context("não foi possível criar a trava de operação"),
            }
        }
        bail!("não foi possível obter a trava de operação")
    }
}

impl Drop for OpLock {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
            if *p == lock_path_raw() {
                HELD.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }
}

/// Root (Unix) ou administrador (Windows).
pub fn is_privileged() -> bool {
    #[cfg(unix)]
    {
        is_root()
    }
    #[cfg(windows)]
    {
        // SAFETY: função Win32 sem argumentos.
        unsafe { crate::win32::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Início de uma operação: com privilégio, pega a trava; sem, só confere que está livre.
pub fn guard_operation() -> Result<Option<OpLock>> {
    if is_privileged() {
        Ok(Some(OpLock::acquire()?))
    } else {
        OpLock::check_free()?;
        Ok(None)
    }
}

/// Os subcomandos internos só fazem sentido já elevados.
pub fn require_privileged() -> Result<()> {
    if is_privileged() {
        return Ok(());
    }
    if cfg!(windows) {
        bail!("Subcomando interno requer privilégios de administrador.");
    }
    bail!(
        "Subcomando interno requer privilégios de root (use 'guialar apply' ou 'guialar revert')."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uid_efetivo_vem_da_terceira_coluna() {
        let s = "Name:\tx\nUid:\t1000\t0\t1000\t1000\n";
        assert_eq!(effective_uid(s), Some(0));
        assert_eq!(effective_uid("Uid:\t0\t0\t0\t0"), Some(0));
        assert_eq!(effective_uid("sem uid"), None);
    }

    #[test]
    fn elevacao_escolhe_ferramenta_certa() {
        assert!(privilege_prefix(true, true, true, true).is_empty());
        assert_eq!(privilege_prefix(false, true, true, true), ["pkexec"]);
        assert_eq!(privilege_prefix(false, false, true, true), ["sudo"]);
        assert_eq!(privilege_prefix(false, false, true, false), ["pkexec"]);
    }

    #[test]
    fn home_do_passwd() {
        let p = "root:x:0:0::/root:/bin/sh\nqa:x:1000:1000::/home/qa:/bin/sh\n";
        assert_eq!(home_from_passwd(p, "qa"), Some(PathBuf::from("/home/qa")));
        assert_eq!(home_from_passwd(p, "zz"), None);
    }

    #[test]
    fn command_exists_acha_e_nao_acha() {
        assert!(command_exists(if cfg!(windows) { "cmd" } else { "sh" }));
        assert!(!command_exists("comando-que-nao-existe-xyz"));
        assert!(!command_exists(""));
    }

    #[test]
    fn encode_powershell_confere_com_referencia() {
        assert_eq!(encode_powershell("A"), "QQA=");
        assert_eq!(encode_powershell("Get-Date"), "RwBlAHQALQBEAGEAdABlAA==");
        assert_eq!(encode_powershell(""), "");
    }

    #[test]
    fn alias_perigoso_rejeitado_e_acentuado_aceito() {
        for ruim in ["Wi-Fi'; calc; '", "x$(y)", "a`b", "a|b", "a&b", "a\nb", ""] {
            assert!(!is_safe_ps_alias(ruim), "{ruim:?}");
        }
        assert!(is_safe_ps_alias("Conexão Local"));
        assert!(is_safe_ps_alias("Ethernet (Realtek PCIe)"));
    }

    #[test]
    fn trava_impede_operacao_paralela() {
        let f = std::env::temp_dir().join(format!("guialar-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let a = OpLock::acquire_in(&f).unwrap();
        assert!(OpLock::acquire_in(&f).is_err());
        drop(a);
        assert!(OpLock::acquire_in(&f).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn appimage_so_vale_com_o_executavel_dentro_do_appdir() {
        use std::ffi::OsStr;
        let ai = Some(OsStr::new("/home/u/GuiaLar.AppImage"));
        let appdir = Some(OsStr::new("/tmp/.mount_GuiaLaXYZ"));
        let dentro = Path::new("/tmp/.mount_GuiaLaXYZ/usr/bin/guialar");
        assert_eq!(
            running_appimage_from(ai, appdir, dentro),
            Some(PathBuf::from("/home/u/GuiaLar.AppImage"))
        );
        // APPIMAGE herdado de OUTRO AppImage: o executável não está no APPDIR dele
        let build = Path::new("/home/u/gl/target/release/guialar");
        assert_eq!(running_appimage_from(ai, appdir, build), None);
        // prefixo de texto não é pasta: `.mount_GuiaLaXYZ2` não está em `.mount_GuiaLaXYZ`
        let vizinho = Path::new("/tmp/.mount_GuiaLaXYZ2/usr/bin/guialar");
        assert_eq!(running_appimage_from(ai, appdir, vizinho), None);
        // faltando uma das duas variáveis, vazias ou APPDIR relativo
        assert_eq!(running_appimage_from(None, appdir, dentro), None);
        assert_eq!(running_appimage_from(ai, None, dentro), None);
        assert_eq!(
            running_appimage_from(Some(OsStr::new("")), appdir, dentro),
            None
        );
        assert_eq!(
            running_appimage_from(ai, Some(OsStr::new("")), dentro),
            None
        );
        assert_eq!(
            running_appimage_from(ai, Some(OsStr::new("rel/dir")), Path::new("rel/dir/x")),
            None
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn trava_de_producao_fica_em_run_e_nao_depende_do_tmpdir() {
        // `lock_path_raw` fora do Sandbox: nunca `temp_dir()`, mesmo com TMPDIR trocado
        assert_eq!(lock_path_raw(), PathBuf::from("/run/guialar/op.lock"));
        let antes = std::env::var_os("TMPDIR");
        std::env::set_var("TMPDIR", "/tmp/outro-tmpdir");
        let depois = lock_path_raw();
        match antes {
            Some(v) => std::env::set_var("TMPDIR", v),
            None => std::env::remove_var("TMPDIR"),
        }
        assert_eq!(depois, PathBuf::from("/run/guialar/op.lock"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn trava_nao_segue_link_e_e_legivel_por_todos() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("gl-lock-nf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let alvo = d.join("alvo-do-link");
        std::fs::write(&alvo, "intacto").unwrap();
        // alvo «velho» (mais de 120 s): a versão que seguia o link o trataria como trava
        // abandonada, apagaria o link e passaria; a nova recusa, sem depender do relógio
        let velho = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
        std::fs::File::options()
            .write(true)
            .open(&alvo)
            .unwrap()
            .set_modified(velho)
            .unwrap();
        let trava = d.join("op.lock");
        std::os::unix::fs::symlink(&alvo, &trava).unwrap();
        let e = OpLock::acquire_in(&trava).err().expect("recusa o link");
        assert!(format!("{e:#}").contains("link"), "{e:#}");
        assert!(
            std::fs::symlink_metadata(&trava)
                .unwrap()
                .file_type()
                .is_symlink(),
            "o link não foi apagado"
        );
        assert_eq!(std::fs::read_to_string(&alvo).unwrap(), "intacto");
        std::fs::remove_file(&trava).unwrap();
        // criada pelo processo, com 0644 (o usuário só lê)
        let l = OpLock::acquire_in(&trava).unwrap();
        let mode = std::fs::metadata(&trava).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        drop(l);
        std::fs::remove_dir_all(&d).ok();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn umask_fechada_do_root_nao_esconde_a_trava_do_usuario() {
        use std::os::unix::fs::PermissionsExt;
        extern "C" {
            fn umask(mask: u32) -> u32;
        }
        let d = std::env::temp_dir().join(format!("gl-lock-umask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let trava = d.join("op.lock");
        // SAFETY: só troca e restaura a máscara do processo (teste de um só fio do Sandbox).
        let antiga = unsafe { umask(0o077) };
        let l = OpLock::acquire_in(&trava);
        // SAFETY: restaura a máscara.
        unsafe { umask(antiga) };
        let l = l.unwrap();
        assert_eq!(
            std::fs::metadata(&trava).unwrap().permissions().mode() & 0o777,
            0o644
        );
        drop(l);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn usuario_real_por_sudo_e_pkexec() {
        let p = "root:x:0:0::/root:/bin/sh\nqa:x:1000:1001::/home/qa:/bin/sh\n";
        let e = caller_from_env(Some("1000"), Some("1001"), Some("qa"), None, p).unwrap();
        assert_eq!((e.1, e.2, e.3), (1000, 1001, PathBuf::from("/home/qa")));
        let e = caller_from_env(None, None, None, Some("1000"), p).unwrap();
        assert_eq!((e.0.as_str(), e.2), ("qa", 1001));
        let e = caller_from_env(None, None, Some("qa"), None, p).unwrap();
        assert_eq!(e.1, 1000);
        // root nunca é "o chamador"
        assert!(caller_from_env(Some("0"), Some("0"), Some("root"), None, p).is_none());
        assert!(caller_from_env(None, None, None, None, p).is_none());
        assert!(caller_from_env(None, None, None, Some("4242"), p).is_none());
    }

    #[test]
    fn trava_orfa_e_substituida() {
        let base = std::env::temp_dir().join(format!("gl-orfa-{}", std::process::id()));
        let _ = std::fs::remove_file(&base);
        // vazio (touch) com > 50 ms
        std::fs::write(&base, "").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(80));
        assert!(lock_is_stale(&base));
        let l = OpLock::acquire_in(&base).unwrap();
        assert_eq!(
            std::fs::read_to_string(&base).unwrap().trim(),
            std::process::id().to_string()
        );
        // dono vivo (este processo): não é stale; segunda aquisição falha
        std::thread::sleep(std::time::Duration::from_millis(80));
        assert!(!lock_is_stale(&base));
        assert!(OpLock::acquire_in(&base).is_err());
        drop(l);
        // PID inexistente (kill -9)
        std::fs::write(&base, "999999999").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(80));
        #[cfg(target_os = "linux")]
        {
            assert!(lock_is_stale(&base));
            assert!(OpLock::acquire_in(&base).is_ok());
        }
        // recém-criado e vazio ainda não é stale (corrida com a gravação do PID)
        let _ = std::fs::remove_file(&base);
        std::fs::write(&base, "").unwrap();
        assert!(!lock_is_stale(&base));
        let _ = std::fs::remove_file(&base);
    }
}

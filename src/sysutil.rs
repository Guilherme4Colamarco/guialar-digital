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
    std::env::temp_dir().join("guialar-op.lock")
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

impl OpLock {
    pub fn acquire() -> Result<Self> {
        if HELD.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(Self(None)); // já seguro por este processo
        }
        let l = Self::acquire_in(&lock_path())?;
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
        bail!(LOCK_BUSY)
    }

    pub fn acquire_in(path: &Path) -> Result<Self> {
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(mut f) => {
                    use std::io::Write;
                    let _ = write!(f, "{}", std::process::id());
                    let _ = f.flush();
                    return Ok(Self(Some(path.to_path_buf())));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
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
            if *p == lock_path() {
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

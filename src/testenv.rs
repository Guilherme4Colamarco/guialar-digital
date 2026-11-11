//! Ambiente isolado para testes que mexem em $HOME/%LOCALAPPDATA%, na trava de operação
//! e no modo de filtro (tudo global ao processo): um só teste por vez.
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LOCK_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Pasta da trava de operação neste teste (só dentro de um `Sandbox`). Evita mexer em
/// TMPDIR/TMP/TEMP, que são globais e quebrariam testes concorrentes que não usam o lock.
pub fn lock_dir() -> Option<PathBuf> {
    LOCK_DIR.with(|d| d.borrow().clone())
}

/// Esta thread de teste está dentro de um `Sandbox`?
pub fn sandboxed() -> bool {
    ACTIVE.with(|a| a.get())
}

/// O caminho está dentro do `Sandbox` desta thread? (Falha alto se não.)
pub fn assert_inside(path: &Path, what: &str) {
    let root = lock_dir().and_then(|d| d.parent().map(Path::to_path_buf));
    assert!(
        root.is_some_and(|r| path.starts_with(&r)),
        "teste tocou {what} fora do Sandbox: {path:?}"
    );
}

/// Guarda (só em `cargo test`): quem toca o manifesto ou a trava de operação precisa estar
/// num `Sandbox`; senão o teste gravaria no estado REAL da máquina (`%LOCALAPPDATA%`, `$HOME`,
/// pasta temporária). Falha alto em vez de sujar a máquina.
pub fn assert_sandboxed(what: &str) {
    assert!(
        sandboxed(),
        "teste tocou o estado real ({what}) sem testenv::Sandbox: use Sandbox::new(..)"
    );
}

/// Trava o ambiente global; um teste que falhou antes não envenena os outros.
pub fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const VARS: [&str; 7] = [
    "HOME",
    "LOCALAPPDATA",
    "APPIMAGE",
    "SUDO_UID",
    "SUDO_GID",
    "SUDO_USER",
    "PKEXEC_UID",
];

/// Pasta de estado temporária: `HOME` e `LOCALAPPDATA` apontam para ela e a trava de
/// operação fica em `<pasta>/tmp`; modo de filtro volta para `family`. Restaura tudo ao sair.
pub struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    dir: PathBuf,
    old: Vec<(&'static str, Option<OsString>)>,
}

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        Self::build(tag, &[])
    }

    /// Como `new`, mas antes define `preset` (ex.: `SUDO_USER`) para provar que o Sandbox
    /// limpa um ambiente de sudo/pkexec em vez de herdá-lo.
    pub fn with_preset(tag: &str, preset: &[(&str, &str)]) -> Self {
        Self::build(tag, preset)
    }

    fn build(tag: &str, preset: &[(&str, &str)]) -> Self {
        let guard = lock();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("guialar-sbx-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        let old = VARS.iter().map(|v| (*v, std::env::var_os(v))).collect();
        for (k, v) in preset {
            std::env::set_var(k, v);
        }
        std::env::set_var("HOME", &dir);
        std::env::set_var("LOCALAPPDATA", &dir);
        LOCK_DIR.with(|d| *d.borrow_mut() = Some(dir.join("tmp")));
        // Rodando como root via sudo/pkexec, o home sairia do /etc/passwd e não do HOME.
        for v in [
            "APPIMAGE",
            "SUDO_UID",
            "SUDO_GID",
            "SUDO_USER",
            "PKEXEC_UID",
        ] {
            std::env::remove_var(v);
        }
        crate::config::set_mode(crate::config::FilterMode::Family);
        ACTIVE.with(|a| a.set(true));
        Self {
            _guard: guard,
            dir,
            old,
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Caminho do manifesto neste ambiente.
    pub fn manifest_path(&self) -> PathBuf {
        crate::config::DnsStateManifest::file_path().unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        ACTIVE.with(|a| a.set(false));
        LOCK_DIR.with(|d| *d.borrow_mut() = None);
        for (v, val) in &self.old {
            match val {
                Some(x) => std::env::set_var(v, x),
                None => std::env::remove_var(v),
            }
        }
        crate::config::set_mode(crate::config::FilterMode::Family);
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_aponta_manifesto_e_trava_para_a_pasta_temporaria() {
        let s = Sandbox::new("guarda");
        assert!(sandboxed());
        let m = s.manifest_path();
        assert!(m.starts_with(s.dir()), "{m:?} fora de {:?}", s.dir());
        assert!(crate::sysutil::lock_path_for_test().starts_with(s.dir()));
    }

    #[test]
    fn ambiente_de_sudo_ou_pkexec_nao_escapa_do_sandbox() {
        let preset = [
            ("SUDO_UID", "1000"),
            ("SUDO_GID", "1000"),
            ("SUDO_USER", "nobody"),
            ("PKEXEC_UID", "1000"),
        ];
        let s = Sandbox::with_preset("sudo", &preset);
        for (k, _) in preset {
            assert!(std::env::var_os(k).is_none(), "{k} deveria ter sido limpo");
        }
        assert!(crate::sysutil::effective_home()
            .unwrap()
            .starts_with(s.dir()));
        assert!(s.manifest_path().starts_with(s.dir()));
        drop(s);
        // ao sair, volta o ambiente de ANTES do Sandbox: o preset do teste não vaza
        assert_ne!(std::env::var("SUDO_USER").as_deref(), Ok("nobody"));
        assert_ne!(std::env::var("PKEXEC_UID").as_deref(), Ok("1000"));
    }

    #[test]
    fn fora_do_sandbox_o_guarda_nega() {
        // o teste não pode estar dentro de um Sandbox (a flag é por thread)
        assert!(!sandboxed());
        let r = std::panic::catch_unwind(|| assert_sandboxed("teste"));
        assert!(r.is_err());
        let r = std::panic::catch_unwind(|| {
            let _ = crate::config::DnsStateManifest::file_path();
        });
        assert!(r.is_err(), "file_path() fora do Sandbox deve falhar");
        let r = std::panic::catch_unwind(|| {
            let _ = crate::sysutil::OpLock::check_free();
        });
        assert!(r.is_err(), "a trava fora do Sandbox deve falhar");
    }
}

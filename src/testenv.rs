//! Ambiente isolado para testes que mexem em $HOME/%LOCALAPPDATA%, na pasta temporária
//! e no modo de filtro (tudo global ao processo): um só teste por vez.
#![allow(dead_code)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Trava o ambiente global; um teste que falhou antes não envenena os outros.
pub fn lock() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const VARS: [&str; 6] = ["HOME", "LOCALAPPDATA", "TMPDIR", "TMP", "TEMP", "APPIMAGE"];

/// Pasta de estado temporária: `HOME`, `LOCALAPPDATA` e a pasta temporária (trava de
/// operação) apontam para ela; modo de filtro volta para `family`. Restaura tudo ao sair.
pub struct Sandbox {
    _guard: MutexGuard<'static, ()>,
    dir: PathBuf,
    old: Vec<(&'static str, Option<OsString>)>,
}

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        let guard = lock();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("guialar-sbx-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("tmp")).unwrap();
        let old = VARS.iter().map(|v| (*v, std::env::var_os(v))).collect();
        std::env::set_var("HOME", &dir);
        std::env::set_var("LOCALAPPDATA", &dir);
        for v in ["TMPDIR", "TMP", "TEMP"] {
            std::env::set_var(v, dir.join("tmp"));
        }
        std::env::remove_var("APPIMAGE");
        crate::config::set_mode(crate::config::FilterMode::Family);
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

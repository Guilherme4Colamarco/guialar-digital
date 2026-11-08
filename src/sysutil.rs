//! Utilitários de sistema compartilhados: elevação, home efetivo e PATH.
#![cfg_attr(not(unix), allow(dead_code))]

use std::path::{Path, PathBuf};

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

/// Home efetivo para o manifesto: sob `sudo`, o do usuário que chamou (não `/root`).
pub fn effective_home() -> Option<PathBuf> {
    if is_root() {
        if let Ok(user) = std::env::var("SUDO_USER") {
            if user != "root" {
                if let Some(h) = std::fs::read_to_string("/etc/passwd")
                    .ok()
                    .and_then(|p| home_from_passwd(&p, &user))
                {
                    return Some(h);
                }
            }
        }
    }
    std::env::var_os("HOME").map(PathBuf::from)
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
}

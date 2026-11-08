//! Utilitários de sistema compartilhados: elevação, home efetivo e PATH.
#![cfg_attr(not(unix), allow(dead_code))]
#![cfg_attr(unix, allow(dead_code))]

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
}

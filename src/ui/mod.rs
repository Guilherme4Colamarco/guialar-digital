//! Interface gráfica nativa: GTK4/Libadwaita (Linux) e Win32/Fluent (Windows 11).
//!
//! As janelas só chamam as funções deste módulo; elas reaproveitam o mesmo núcleo
//! da CLI (`platform`, `verify`, `browser`, `policy`).
#![cfg_attr(
    not(any(windows, all(target_os = "linux", target_env = "gnu"))),
    allow(dead_code)
)]

pub mod theme;
#[allow(unused_imports)]
pub use theme::{actions_for, UiState};

#[cfg(target_os = "linux")]
mod linux_gtk;
#[cfg(target_os = "windows")]
mod windows_winui;

use crate::config::DnsStateManifest;
use anyhow::Result;
use theme::Snapshot;

/// Há ambiente gráfico para abrir a janela?
pub fn gui_available() -> bool {
    if std::env::var_os("GUIALAR_NO_GUI").is_some() {
        return false;
    }
    if cfg!(windows) {
        return true;
    }
    has_display(
        std::env::var_os("DISPLAY").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
    )
}

pub fn has_display(display: Option<&std::ffi::OsStr>, wayland: Option<&std::ffi::OsStr>) -> bool {
    [display, wayland]
        .iter()
        .any(|v| v.is_some_and(|s| !s.is_empty()))
}

/// Abre a janela. `Ok(false)` = sem suporte/sem display: o chamador usa a CLI.
pub fn run() -> Result<bool> {
    if !gui_available() {
        return Ok(false);
    }
    #[cfg(target_os = "linux")]
    {
        linux_gtk::run()
    }
    #[cfg(target_os = "windows")]
    {
        windows_winui::run().map(|_| true)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(false)
    }
}

/// Lê o estado atual (pode demorar até o timeout do teste DNS: chame fora da thread de UI).
pub fn snapshot() -> Snapshot {
    let mode = crate::verify::active_mode();
    // Modo só anúncios não mexe nos navegadores: nada "falta".
    let missing = if mode == Some(crate::config::FilterMode::AdsOnly) {
        Vec::new()
    } else {
        crate::policy::browsers_without_blocker()
    };
    Snapshot {
        mode,
        status: crate::verify::check_status(),
        browsers: crate::browser::detect_browsers()
            .into_iter()
            .map(|b| (b.name, b.is_installed))
            .collect(),
        missing_blocker: missing,
        manifest_path: DnsStateManifest::file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        backend: backend_name(),
        configured_dns: crate::platform::configured_dns(),
    }
}

fn backend_name() -> String {
    #[cfg(target_os = "linux")]
    {
        match crate::platform::linux::detect_active_backend() {
            crate::config::BackendKind::NetworkManager => "NetworkManager".into(),
            crate::config::BackendKind::SystemdResolved => "systemd-resolved".into(),
            crate::config::BackendKind::OpenResolvRunit => "runit / resolvconf".into(),
            crate::config::BackendKind::DirectResolvConf => "/etc/resolv.conf".into(),
            crate::config::BackendKind::WindowsPowerShell => "PowerShell".into(),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "Windows (PowerShell)".into()
    }
}

/// Ativa o DNS seguro e o bloqueador de anúncios.
pub fn action_activate() -> Result<String> {
    let mode = crate::config::current_mode();
    if DnsStateManifest::load()?.is_some_and(|m| m.mode == mode) {
        return Ok("A proteção já está ativa. Use \"Desativar\" antes de ativar de novo.".into());
    }
    // Uma única elevação (um só UAC / uma só senha). No Linux não abre o navegador por
    // cima da janela de senha: o bloqueador é um passo à parte.
    crate::platform::apply_all()?;
    Ok(match (mode, cfg!(windows)) {
        (crate::config::FilterMode::AdsOnly, _) => {
            "Anúncios e rastreadores bloqueados. Os navegadores não foram alterados.".into()
        }
        (_, true) => {
            "Proteção ativada. Bloqueador de anúncios configurado. Reabra o navegador.".into()
        }
        (_, false) => "Proteção ativada.".into(),
    })
}

/// Instala/abre o bloqueador de anúncios nos navegadores.
pub fn action_browsers() -> Result<String> {
    crate::handle_browser(true)?;
    Ok("Bloqueador de anúncios configurado. Reabra o navegador.".into())
}

/// O arquivo existe mas não é um manifesto válido.
fn is_corrupt_manifest(path: &std::path::Path) -> bool {
    // Bytes (não texto): UTF-8 inválido também é "corrompido".
    std::fs::read(path)
        .map(|b| serde_json::from_slice::<DnsStateManifest>(&b).is_err())
        .unwrap_or(false)
}

/// Desfaz DNS (e políticas do uBlock no Windows).
pub fn action_restore() -> Result<String> {
    // Manifesto ilegível: o fluxo de reversão limpa o arquivo e volta o DNS ao automático.
    let corrompido = DnsStateManifest::file_path()
        .map(|p| is_corrupt_manifest(&p))
        .unwrap_or(false);
    crate::handle_revert()?;
    if corrompido {
        Ok("O registro da proteção estava danificado. O DNS voltou ao padrão da rede; clique em \"Proteger este computador\" para começar de novo.".into())
    } else {
        Ok("Configurações de rede restauradas.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn display_detectado() {
        assert!(!has_display(None, None));
        assert!(!has_display(Some(OsStr::new("")), Some(OsStr::new(""))));
        assert!(has_display(Some(OsStr::new(":0")), None));
        assert!(has_display(None, Some(OsStr::new("wayland-0"))));
    }

    /// E3: com o arquivo de estado corrompido, "Desativar" não pode ficar sem saída.
    #[test]
    fn manifesto_corrompido_e_removido() {
        let dir = std::env::temp_dir().join(format!("guialar-ui-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("state.json");
        std::fs::write(&f, "{ quebrado").unwrap();
        assert!(is_corrupt_manifest(&f));
        std::fs::write(&f, [0xff, 0xfe, 0x00, 0x80]).unwrap();
        assert!(is_corrupt_manifest(&f));
        std::fs::write(&f, r#"{"backend":"DirectResolvConf","applied_at_utc":"1","connections":[],"previous_resolv_conf":null}"#).unwrap();
        assert!(!is_corrupt_manifest(&f));
        // Sem permissão de leitura NÃO é corrompido.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&f, "{ lixo").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
            let readable = std::fs::read(&f).is_ok(); // root lê mesmo assim
            if !readable {
                assert!(!is_corrupt_manifest(&f));
            }
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}

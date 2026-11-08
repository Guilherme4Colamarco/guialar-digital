//! Interface gráfica nativa: GTK4/Libadwaita (Linux) e Win32/Fluent (Windows 11).
//!
//! As janelas só chamam as funções deste módulo; elas reaproveitam o mesmo núcleo
//! da CLI (`platform`, `verify`, `browser`, `policy`).
#![cfg_attr(
    not(any(windows, all(target_os = "linux", target_env = "gnu"))),
    allow(dead_code)
)]

pub mod theme;

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
    Snapshot {
        status: crate::verify::check_status(),
        browsers: crate::browser::detect_browsers()
            .into_iter()
            .map(|b| (b.name, b.is_installed))
            .collect(),
        manifest_path: DnsStateManifest::file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        backend: backend_name(),
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
    crate::platform::apply_dns()?;
    let extra = action_browsers().unwrap_or_else(|e| format!("Navegadores: {e:#}"));
    Ok(format!("Proteção ativada. {extra}"))
}

/// Instala/abre o bloqueador de anúncios nos navegadores.
pub fn action_browsers() -> Result<String> {
    crate::handle_browser(true)?;
    Ok("Bloqueador de anúncios configurado. Reabra o navegador.".into())
}

/// Desfaz DNS (e políticas do uBlock no Windows).
pub fn action_restore() -> Result<String> {
    crate::handle_revert()?;
    Ok("Configurações de rede restauradas.".into())
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
}

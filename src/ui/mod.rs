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
    let mode = crate::verify::active_mode();
    // Modo só anúncios não mexe nos navegadores: nada "falta".
    let missing = if mode == Some(crate::config::FilterMode::AdsOnly) {
        Vec::new()
    } else {
        crate::policy::browsers_without_blocker()
    };
    let status = crate::verify::check_status();
    // Filtro do GuiaLar ainda na rede, mas sem registro nosso (manifesto apagado).
    let filter_without_record = matches!(status, crate::verify::ProtectionStatus::Unprotected)
        && crate::platform::filter_dns_in_use();
    Snapshot {
        mode,
        filter_without_record,
        status,
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
    use crate::protection::Protected;
    let mode = crate::config::current_mode();
    // Uma única elevação (um só UAC / uma só senha). No Linux não abre o navegador por
    // cima da janela de senha: o bloqueador é um passo à parte.
    if crate::protection::protect(mode, true, |_| {})? == Protected::AlreadyActive {
        return Ok("A proteção já está ativa. Use \"Desativar\" antes de ativar de novo.".into());
    }
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

/// Desfaz DNS (e políticas do uBlock no Windows).
pub fn action_restore() -> Result<String> {
    use crate::protection::Unprotected;
    Ok(match crate::protection::unprotect()? {
        Unprotected::ResetCorruptRecord => "O registro da proteção estava danificado. O DNS voltou ao padrão da rede; clique em \"Proteger este computador\" para começar de novo.",
        Unprotected::ResetWithoutRecord => "Não havia registro da proteção. O DNS voltou ao padrão da rede.",
        Unprotected::NothingToUndo => "Não havia proteção ativa para desfazer.",
        Unprotected::Restored => "Configurações de rede restauradas.",
    }
    .into())
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

//! Textos e cores compartilhados pelas interfaces nativas.
#![cfg_attr(
    not(any(windows, all(target_os = "linux", target_env = "gnu"))),
    allow(dead_code)
)]
#![cfg_attr(all(target_os = "linux", target_env = "gnu"), allow(dead_code))]

use crate::verify::ProtectionStatus;

/// `true` se o nome de um tema GTK (ex.: `Adwaita-dark`, `Arc-Darker`) é escuro.
#[cfg_attr(windows, allow(dead_code))]
pub fn gtk_theme_is_dark(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("dark") || n.contains("black")
}

/// `gtk-application-prefer-dark-theme=1` ou `gtk-theme-name` escuro em um `settings.ini` do GTK.
#[cfg_attr(windows, allow(dead_code))]
pub fn ini_prefers_dark(content: &str) -> bool {
    content.lines().any(|l| {
        let Some((k, v)) = l.split_once('=') else {
            return false;
        };
        let (k, v) = (k.trim(), v.trim());
        (k == "gtk-application-prefer-dark-theme" && matches!(v, "1" | "true" | "True"))
            || (k == "gtk-theme-name" && gtk_theme_is_dark(v))
    })
}

/// Junta as fontes de tema do desktop Linux: `GTK_THEME`, GSettings (`color-scheme`,
/// `gtk-theme`) e `settings.ini`.
#[cfg_attr(windows, allow(dead_code))]
pub fn desktop_prefers_dark(
    env_theme: Option<&str>,
    color_scheme: Option<&str>,
    gtk_theme: Option<&str>,
    ini: Option<&str>,
) -> bool {
    env_theme.is_some_and(gtk_theme_is_dark)
        || color_scheme == Some("prefer-dark")
        || gtk_theme.is_some_and(gtk_theme_is_dark)
        || ini.is_some_and(ini_prefers_dark)
}

/// Fotografia do sistema exibida na janela.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: ProtectionStatus,
    /// (nome, instalado)
    pub browsers: Vec<(&'static str, bool)>,
    #[cfg_attr(windows, allow(dead_code))]
    pub manifest_path: String,
    /// Backend de DNS detectado (só Linux).
    #[cfg_attr(windows, allow(dead_code))]
    pub backend: String,
}

pub fn headline(status: &ProtectionStatus) -> &'static str {
    match status {
        ProtectionStatus::Protected => "Proteção Familiar Ativa",
        ProtectionStatus::PartiallyConfigured(_) => "Configuração Parcial",
        ProtectionStatus::Unprotected => "Computador Desprotegido",
    }
}

pub fn description(status: &ProtectionStatus) -> String {
    match status {
        ProtectionStatus::Protected => {
            "Filtro Cloudflare for Families ativo (1.1.1.3 / 1.0.0.3): bloqueia malware e conteúdo adulto."
                .into()
        }
        ProtectionStatus::PartiallyConfigured(msg) => msg.clone(),
        ProtectionStatus::Unprotected => {
            "Este computador usa o DNS padrão da rede. Ative a proteção para filtrar malware e conteúdo adulto."
                .into()
        }
    }
}

pub fn browsers_summary(browsers: &[(&'static str, bool)]) -> String {
    let found: Vec<&str> = browsers.iter().filter(|b| b.1).map(|b| b.0).collect();
    if found.is_empty() {
        "Nenhum navegador compatível encontrado".into()
    } else {
        found.join(", ")
    }
}

/// Cores `0xRRGGBB` do tema Fluent (Windows 11).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: u32,
    pub card: u32,
    pub border: u32,
    pub text: u32,
    pub text_secondary: u32,
    pub ok: u32,
    pub warn: u32,
    pub bad: u32,
}

pub const LIGHT: Palette = Palette {
    background: 0xF3F3F3,
    card: 0xFFFFFF,
    border: 0xE5E5E5,
    text: 0x1A1A1A,
    text_secondary: 0x5D5D5D,
    ok: 0x0F7B0F,
    warn: 0x9D5D00,
    bad: 0xC42B1C,
};

pub const DARK: Palette = Palette {
    background: 0x202020,
    card: 0x2B2B2B,
    border: 0x3A3A3A,
    text: 0xFFFFFF,
    text_secondary: 0xC5C5C5,
    ok: 0x6CCB5F,
    warn: 0xFCE100,
    bad: 0xFF99A4,
};

pub fn status_color(p: &Palette, status: &ProtectionStatus) -> u32 {
    match status {
        ProtectionStatus::Protected => p.ok,
        ProtectionStatus::PartiallyConfigured(_) => p.warn,
        ProtectionStatus::Unprotected => p.bad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titulos_por_estado() {
        assert_eq!(
            headline(&ProtectionStatus::Protected),
            "Proteção Familiar Ativa"
        );
        assert_eq!(
            headline(&ProtectionStatus::Unprotected),
            "Computador Desprotegido"
        );
        assert_eq!(
            headline(&ProtectionStatus::PartiallyConfigured("x".into())),
            "Configuração Parcial"
        );
    }

    #[test]
    fn resumo_de_navegadores() {
        assert_eq!(
            browsers_summary(&[("Firefox", false)]),
            "Nenhum navegador compatível encontrado"
        );
        assert_eq!(
            browsers_summary(&[("Firefox", true), ("Brave", false), ("Edge", true)]),
            "Firefox, Edge"
        );
    }

    #[test]
    fn nome_de_tema_gtk_escuro_ou_claro() {
        assert!(gtk_theme_is_dark("Adwaita-dark"));
        assert!(gtk_theme_is_dark("Arc-DARKER"));
        assert!(gtk_theme_is_dark("Yaru:Black"));
        assert!(!gtk_theme_is_dark("Adwaita"));
        assert!(!gtk_theme_is_dark(""));
    }

    #[test]
    fn hkey_current_user_estende_o_sinal() {
        assert_eq!(
            0x8000_0001u32 as i32 as i64 as u64,
            0xFFFF_FFFF_8000_0001u64
        );
        assert_ne!(0x8000_0001u32 as u64, 0xFFFF_FFFF_8000_0001u64);
    }

    #[test]
    fn fontes_de_tema_do_desktop() {
        assert!(desktop_prefers_dark(Some("Adwaita:dark"), None, None, None));
        assert!(desktop_prefers_dark(None, Some("prefer-dark"), None, None));
        assert!(desktop_prefers_dark(
            None,
            Some("default"),
            Some("Arc-Dark"),
            None
        ));
        assert!(desktop_prefers_dark(
            None,
            None,
            None,
            Some("[Settings]\ngtk-application-prefer-dark-theme = 1\n")
        ));
        assert!(!desktop_prefers_dark(
            None,
            Some("default"),
            Some("Adwaita"),
            Some("[Settings]\ngtk-theme-name=Adwaita\n")
        ));
        assert!(!desktop_prefers_dark(None, None, None, None));
    }
}

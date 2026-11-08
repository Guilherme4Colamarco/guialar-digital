//! Módulo de detecção de navegadores e instalação assistida do uBlock Origin.

use std::process::Command;

#[derive(Debug, Clone)]
pub struct BrowserInfo {
    pub name: &'static str,
    pub is_installed: bool,
    pub extension_store_url: &'static str,
}

pub const UBLOCK_FIREFOX: &str = "https://addons.mozilla.org/pt-BR/firefox/addon/ublock-origin/";
pub const UBLOCK_CHROME: &str =
    "https://chromewebstore.google.com/detail/ublock-origin/cjpalhdlnbpafiamejdnhcphjbkeiagm";
pub const UBLOCK_EDGE: &str = "https://microsoftedge.microsoft.com/addons/detail/ublock-origin/odfafepnkmbhccpbejgmiehpchacaeak";

pub fn detect_browsers() -> Vec<BrowserInfo> {
    vec![
        BrowserInfo {
            name: "Firefox",
            is_installed: is_app_installed("firefox"),
            extension_store_url: UBLOCK_FIREFOX,
        },
        BrowserInfo {
            name: "Google Chrome",
            is_installed: is_app_installed("google-chrome")
                || is_app_installed("google-chrome-stable"),
            extension_store_url: UBLOCK_CHROME,
        },
        BrowserInfo {
            name: "Brave",
            is_installed: is_app_installed("brave") || is_app_installed("brave-browser"),
            extension_store_url: UBLOCK_CHROME,
        },
        BrowserInfo {
            name: "Chromium",
            is_installed: is_app_installed("chromium") || is_app_installed("chromium-browser"),
            extension_store_url: UBLOCK_CHROME,
        },
        BrowserInfo {
            name: "Microsoft Edge",
            is_installed: is_app_installed("microsoft-edge") || is_app_installed("msedge"),
            extension_store_url: UBLOCK_EDGE,
        },
    ]
}

pub fn open_extension_page(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/C", "start", url]).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open").arg(url).spawn()?;
    }

    Ok(())
}

fn is_app_installed(cmd: &str) -> bool {
    crate::sysutil::command_exists(cmd)
}

//! Módulo de detecção de navegadores e instalação assistida do uBlock Origin.

use std::process::Command;

#[derive(Debug, Clone)]
pub struct BrowserInfo {
    pub name: &'static str,
    pub is_installed: bool,
    pub extension_store_url: &'static str,
}

pub const UBLOCK_FIREFOX: &str = "https://addons.mozilla.org/pt-BR/firefox/addon/ublock-origin/";
/// uBlock Origin Lite na Chrome Web Store (Chrome, Chromium, Brave). O uBO clássico (MV2)
/// saiu dessa loja.
pub const UBLOCK_CHROME: &str =
    "https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh";
/// uBlock Origin Lite na loja de complementos do Edge (ID próprio, diferente do da Chrome Web Store).
pub const UBLOCK_EDGE: &str = "https://microsoftedge.microsoft.com/addons/detail/ublock-origin-lite/cimighlppcgcoapaliogpjjdehbnofhn";

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

#[cfg_attr(windows, allow(dead_code))]
pub fn open_extension_page(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd").args(["/C", "start", url]).spawn()?;
    }

    #[cfg(target_os = "linux")]
    {
        let status = Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if !status.success() {
            return Err(std::io::Error::other(format!(
                "xdg-open terminou com {:?}",
                status.code()
            )));
        }
    }

    Ok(())
}

fn is_app_installed(cmd: &str) -> bool {
    crate::sysutil::command_exists(cmd) || windows_install_exists(cmd)
}

/// No Windows os navegadores quase nunca estão no `PATH`: procura nos locais padrão.
#[cfg(windows)]
fn windows_install_exists(cmd: &str) -> bool {
    let relative = match cmd {
        "firefox" => r"Mozilla Firefox\firefox.exe",
        "google-chrome" => r"Google\Chrome\Application\chrome.exe",
        "brave" => r"BraveSoftware\Brave-Browser\Application\brave.exe",
        "msedge" => r"Microsoft\Edge\Application\msedge.exe",
        _ => return false,
    };
    ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .any(|dir| std::path::Path::new(&dir).join(relative).is_file())
}

#[cfg(not(windows))]
fn windows_install_exists(_cmd: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(nome: &str) -> &'static str {
        detect_browsers()
            .into_iter()
            .find(|b| b.name == nome)
            .unwrap_or_else(|| panic!("sem {nome}"))
            .extension_store_url
    }

    #[test]
    fn chrome_chromium_e_brave_abrem_o_ubo_lite_da_chrome_web_store() {
        for nome in ["Google Chrome", "Chromium", "Brave"] {
            assert_eq!(url(nome), UBLOCK_CHROME, "{nome}");
        }
        assert!(UBLOCK_CHROME.contains("/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh"));
        assert!(
            !UBLOCK_CHROME.contains("cjpalhdlnbpafiamejdnhcphjbkeiagm"),
            "uBO MV2 saiu da loja"
        );
    }

    #[test]
    fn edge_abre_o_ubo_lite_da_loja_do_edge() {
        assert_eq!(url("Microsoft Edge"), UBLOCK_EDGE);
        assert!(UBLOCK_EDGE.starts_with("https://microsoftedge.microsoft.com/addons/detail/"));
        assert!(UBLOCK_EDGE.ends_with("/ublock-origin-lite/cimighlppcgcoapaliogpjjdehbnofhn"));
    }

    #[test]
    fn firefox_continua_no_ublock_origin_completo() {
        assert_eq!(url("Firefox"), UBLOCK_FIREFOX);
        assert!(UBLOCK_FIREFOX.contains("/addon/ublock-origin/"));
    }
}

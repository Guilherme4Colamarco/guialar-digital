//! Instalação do bloqueador de anúncios por política de navegador (Windows).
//!
//! Chrome/Edge/Brave leem `ExtensionInstallForcelist` do registro; o Firefox lê
//! `distribution\policies.json`. As funções puras são testáveis em qualquer SO.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// uBlock Origin Lite (MV3): o uBO clássico (MV2) saiu da Chrome Web Store.
pub const UBOL_CHROMIUM_ID: &str = "ddkjiahejlhfcafbddmgiahcphecmpfh";
pub const FIREFOX_UBO_ID: &str = "uBlock0@raymondhill.net";
pub const FIREFOX_UBO_XPI: &str =
    "https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi";
pub const POLICY_VALUE_NAME: &str = "9101";

/// (navegador, chave HKLM, dado `id;update_url`).
pub fn chromium_policies() -> Vec<(&'static str, &'static str, String)> {
    let chrome = "https://clients2.google.com/service/update2/crx";
    let edge = "https://edge.microsoft.com/extensionwebstorebase/v1/crx";
    vec![
        (
            "Google Chrome",
            r"HKLM\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist",
            format!("{UBOL_CHROMIUM_ID};{chrome}"),
        ),
        (
            "Microsoft Edge",
            r"HKLM\SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallForcelist",
            format!("{UBOL_CHROMIUM_ID};{edge}"),
        ),
        (
            "Brave",
            r"HKLM\SOFTWARE\Policies\BraveSoftware\Brave\ExtensionInstallForcelist",
            format!("{UBOL_CHROMIUM_ID};{chrome}"),
        ),
    ]
}

/// Mescla a política do uBlock num `policies.json` existente, preservando o resto.
pub fn merge_firefox_policies(existing: Option<&str>) -> Result<String> {
    let mut root: Value = match existing.map(str::trim).filter(|s| !s.is_empty()) {
        Some(t) => serde_json::from_str(t).context("policies.json existente é inválido")?,
        None => json!({}),
    };
    let settings = root
        .as_object_mut()
        .context("policies.json deve ser um objeto")?
        .entry("policies")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("\"policies\" deve ser um objeto")?
        .entry("ExtensionSettings")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("\"ExtensionSettings\" deve ser um objeto")?;
    settings.insert(
        FIREFOX_UBO_ID.to_string(),
        json!({ "installation_mode": "force_installed", "install_url": FIREFOX_UBO_XPI }),
    );
    Ok(serde_json::to_string_pretty(&root)?)
}

#[cfg(windows)]
pub use win::install_all;

#[cfg(windows)]
mod win {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn find_exe(relative: &str) -> Option<PathBuf> {
        ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|d| PathBuf::from(d).join(relative))
            .find(|p| p.is_file())
    }

    pub fn install_all() -> Result<Vec<String>> {
        let mut done = Vec::new();
        let candidates = [
            ("Google Chrome", r"Google\Chrome\Application\chrome.exe"),
            ("Microsoft Edge", r"Microsoft\Edge\Application\msedge.exe"),
            (
                "Brave",
                r"BraveSoftware\Brave-Browser\Application\brave.exe",
            ),
        ];
        for (name, rel) in candidates {
            if find_exe(rel).is_none() {
                continue;
            }
            if let Some((_, key, data)) = chromium_policies().into_iter().find(|p| p.0 == name) {
                reg_add(key, &data)?;
                done.push(format!(
                    "{name}: uBlock Origin Lite instalado ao reabrir o navegador"
                ));
            }
        }
        if let Some(exe) = find_exe(r"Mozilla Firefox\firefox.exe") {
            let dist = exe
                .parent()
                .context("pasta do Firefox")?
                .join("distribution");
            std::fs::create_dir_all(&dist).context("criando pasta distribution do Firefox")?;
            let file = dist.join("policies.json");
            let current = std::fs::read_to_string(&file).ok();
            std::fs::write(&file, merge_firefox_policies(current.as_deref())?)
                .context("gravando policies.json do Firefox")?;
            done.push("Firefox: uBlock Origin instalado ao reabrir o navegador".into());
        }
        Ok(done)
    }

    fn reg_add(key: &str, data: &str) -> Result<()> {
        let status = Command::new("reg")
            .args([
                "add",
                key,
                "/v",
                POLICY_VALUE_NAME,
                "/t",
                "REG_SZ",
                "/d",
                data,
                "/f",
            ])
            .status()
            .context("falha ao executar reg.exe")?;
        if !status.success() {
            anyhow::bail!("reg add falhou em {key} ({:?})", status.code());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firefox_cria_do_zero() {
        let v: Value = serde_json::from_str(&merge_firefox_policies(None).unwrap()).unwrap();
        assert_eq!(
            v["policies"]["ExtensionSettings"][FIREFOX_UBO_ID]["installation_mode"],
            "force_installed"
        );
    }

    #[test]
    fn firefox_preserva_existente() {
        let atual = r#"{"policies":{"DisableTelemetry":true,"ExtensionSettings":{"x@y":{"installation_mode":"blocked"}}}}"#;
        let v: Value = serde_json::from_str(&merge_firefox_policies(Some(atual)).unwrap()).unwrap();
        assert_eq!(v["policies"]["DisableTelemetry"], true);
        assert_eq!(
            v["policies"]["ExtensionSettings"]["x@y"]["installation_mode"],
            "blocked"
        );
        assert!(v["policies"]["ExtensionSettings"][FIREFOX_UBO_ID].is_object());
    }

    #[test]
    fn firefox_rejeita_json_invalido() {
        assert!(merge_firefox_policies(Some("{nao json")).is_err());
    }

    #[test]
    fn chromium_tres_navegadores() {
        let p = chromium_policies();
        assert_eq!(p.len(), 3);
        assert!(p
            .iter()
            .all(|(_, k, d)| k.starts_with(r"HKLM\SOFTWARE\Policies")
                && d.starts_with(UBOL_CHROMIUM_ID)
                && d.contains(";https://")));
    }
}

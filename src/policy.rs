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

/// Remove só a entrada do GuiaLar de um `policies.json`; `None` = nada a gravar/remover.
pub fn remove_firefox_policy(existing: &str) -> Result<Option<String>> {
    let mut root: Value =
        serde_json::from_str(existing).context("policies.json existente é inválido")?;
    let Some(settings) = root
        .get_mut("policies")
        .and_then(|p| p.get_mut("ExtensionSettings"))
        .and_then(Value::as_object_mut)
    else {
        return Ok(None);
    };
    if settings.remove(FIREFOX_UBO_ID).is_none() {
        return Ok(None);
    }
    if settings.is_empty() {
        root["policies"]
            .as_object_mut()
            .map(|p| p.remove("ExtensionSettings"));
    }
    Ok(Some(serde_json::to_string_pretty(&root)?))
}

/// Navegadores instalados que ainda não receberam o bloqueador (só Windows).
pub fn browsers_without_blocker() -> Vec<String> {
    #[cfg(windows)]
    {
        win::browsers_without_blocker()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[cfg(windows)]
pub use win::{install_all, revert_all};

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

    pub fn browsers_without_blocker() -> Vec<String> {
        let mut out = Vec::new();
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
            let Some((_, key, _)) = chromium_policies().into_iter().find(|p| p.0 == name) else {
                continue;
            };
            let has = hidden(Command::new("reg").args(["query", key, "/v", POLICY_VALUE_NAME]))
                .output()
                .map(|o| {
                    o.status.success()
                        && String::from_utf8_lossy(&o.stdout).contains(UBOL_CHROMIUM_ID)
                })
                .unwrap_or(false);
            if !has {
                out.push(name.to_string());
            }
        }
        if let Some(exe) = find_exe(r"Mozilla Firefox\firefox.exe") {
            let file = exe
                .parent()
                .map(|d| d.join("distribution").join("policies.json"));
            let has = file
                .and_then(|f| std::fs::read_to_string(f).ok())
                .is_some_and(|t| t.contains(FIREFOX_UBO_ID));
            if !has {
                out.push("Firefox".to_string());
            }
        }
        out
    }

    /// Remove as políticas gravadas pelo GuiaLar. Chaves/valores ausentes não são erro.
    pub fn revert_all() -> Result<()> {
        for (_, key, _) in chromium_policies() {
            let existe = hidden(Command::new("reg").args(["query", key, "/v", POLICY_VALUE_NAME]))
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !existe {
                continue;
            }
            let status =
                hidden(Command::new("reg").args(["delete", key, "/v", POLICY_VALUE_NAME, "/f"]))
                    .status()
                    .context("falha ao executar reg.exe")?;
            if !status.success() {
                anyhow::bail!("reg delete falhou em {key} ({:?})", status.code());
            }
        }
        if let Some(exe) = find_exe(r"Mozilla Firefox\firefox.exe") {
            let file = exe
                .parent()
                .context("pasta do Firefox")?
                .join("distribution")
                .join("policies.json");
            if let Ok(current) = std::fs::read_to_string(&file) {
                if let Some(novo) = remove_firefox_policy(&current)? {
                    std::fs::write(&file, novo).context("gravando policies.json do Firefox")?;
                }
            }
        }
        Ok(())
    }

    /// Sem janela de console piscando (CREATE_NO_WINDOW).
    fn hidden(cmd: &mut Command) -> &mut Command {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000)
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

    #[test]
    fn remove_firefox_tira_so_a_entrada_do_guialar() {
        let atual = merge_firefox_policies(Some(
            r#"{"policies":{"DisableTelemetry":true,"ExtensionSettings":{"x@y":{"installation_mode":"blocked"}}}}"#,
        ))
        .unwrap();
        let novo = remove_firefox_policy(&atual).unwrap().unwrap();
        let v: Value = serde_json::from_str(&novo).unwrap();
        assert!(v["policies"]["ExtensionSettings"]
            .get(FIREFOX_UBO_ID)
            .is_none());
        assert_eq!(
            v["policies"]["ExtensionSettings"]["x@y"]["installation_mode"],
            "blocked"
        );
        assert_eq!(v["policies"]["DisableTelemetry"], true);
    }

    #[test]
    fn remove_firefox_sem_entrada_nao_grava() {
        assert!(remove_firefox_policy(r#"{"policies":{}}"#)
            .unwrap()
            .is_none());
        let so_nosso = merge_firefox_policies(None).unwrap();
        let v: Value =
            serde_json::from_str(&remove_firefox_policy(&so_nosso).unwrap().unwrap()).unwrap();
        assert!(v["policies"].get("ExtensionSettings").is_none());
    }
}

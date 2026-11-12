//! Instalação do bloqueador de anúncios por política de navegador (Windows).
//!
//! Chrome/Edge/Brave leem `ExtensionInstallForcelist` do registro; o Firefox lê
//! `distribution\policies.json`. As funções puras são testáveis em qualquer SO.
#![cfg_attr(not(windows), allow(dead_code))]

use anyhow::{Context, Result};
use serde_json::{json, Value};

/// uBlock Origin Lite (MV3): o uBO clássico (MV2) saiu da Chrome Web Store. Cada loja tem o
/// seu ID: o do Chrome/Brave não existe na loja do Edge (o serviço do Edge responde
/// `error-unknownApplication`), e vice-versa.
pub const UBOL_CWS_ID: &str = "ddkjiahejlhfcafbddmgiahcphecmpfh";
pub const UBOL_EDGE_ID: &str = "cimighlppcgcoapaliogpjjdehbnofhn";
pub const CWS_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";
pub const EDGE_UPDATE_URL: &str = "https://edge.microsoft.com/extensionwebstorebase/v1/crx";
pub const FIREFOX_UBO_ID: &str = "uBlock0@raymondhill.net";
pub const FIREFOX_UBO_XPI: &str =
    "https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi";
pub const POLICY_VALUE_NAME: &str = "9101";

/// (navegador, chave HKLM, dado `id;update_url`).
pub fn chromium_policies() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "Google Chrome",
            r"HKLM\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist",
            format!("{UBOL_CWS_ID};{CWS_UPDATE_URL}"),
        ),
        (
            "Microsoft Edge",
            r"HKLM\SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallForcelist",
            format!("{UBOL_EDGE_ID};{EDGE_UPDATE_URL}"),
        ),
        (
            "Brave",
            r"HKLM\SOFTWARE\Policies\BraveSoftware\Brave\ExtensionInstallForcelist",
            format!("{UBOL_CWS_ID};{CWS_UPDATE_URL}"),
        ),
    ]
}

/// Dado que versões anteriores gravaram por engano para este navegador: o Edge recebia o ID
/// da loja do Chrome e não instalava nada. A limpeza ainda o reconhece como nosso.
fn legacy_policy_data(browser: &str) -> Option<String> {
    (browser == "Microsoft Edge").then(|| format!("{UBOL_CWS_ID};{EDGE_UPDATE_URL}"))
}

/// O dado gravado é o que **hoje** instala o bloqueador neste navegador?
pub fn policy_value_covers(browser: &str, data: &str) -> bool {
    chromium_policies()
        .iter()
        .any(|(name, _, expected)| *name == browser && expected == data.trim())
}

/// O que a limpeza faz com o valor `9101` encontrado na chave do navegador.
#[derive(Debug, PartialEq, Eq)]
pub enum PolicyCleanup {
    /// Não há valor `9101`.
    Absent,
    /// É nosso (o atual ou o antigo errado): apagar.
    Remove,
    /// Outro programa ou administrador usou o mesmo nome de valor: não tocar.
    Keep,
}

pub fn cleanup_action(browser: &str, existing: Option<&str>) -> PolicyCleanup {
    let Some(data) = existing.map(str::trim) else {
        return PolicyCleanup::Absent;
    };
    if policy_value_covers(browser, data) || legacy_policy_data(browser).as_deref() == Some(data) {
        PolicyCleanup::Remove
    } else {
        PolicyCleanup::Keep
    }
}

/// Dado `REG_SZ` do valor `name` na saída de `reg query <chave> /v <name>`.
pub fn parse_reg_sz(stdout: &str, name: &str) -> Option<String> {
    stdout.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix(name)?.trim_start();
        let data = rest.strip_prefix("REG_SZ")?;
        Some(data.trim().to_string())
    })
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

/// `true` se o JSON é só `{"policies":{}}`: sobra de uma instalação feita do zero.
pub fn is_empty_policies(json: &str) -> bool {
    serde_json::from_str::<Value>(json)
        .map(|v| v == json!({ "policies": {} }))
        .unwrap_or(false)
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

/// O que o `install_all` faz com a política de um navegador Chromium (E1 do review do #27).
#[derive(Debug, PartialEq, Eq)]
pub enum InstallDecision {
    /// Grava (ou regrava) o valor `9101` do GuiaLar.
    Write,
    /// O `9101` é de outra política: não sobrescreve (o `revert_all` o apagaria depois).
    SkipForeign,
}

pub fn install_decision(browser: &str, existing: Option<&str>) -> InstallDecision {
    if cleanup_action(browser, existing) == PolicyCleanup::Keep {
        InstallDecision::SkipForeign
    } else {
        InstallDecision::Write
    }
}

/// Junta o resultado de cada navegador (E2): uma falha não impede os outros, e o erro final
/// lista o que falhou e diz que o DNS continua aplicado.
pub fn summarize_install(done: Vec<String>, falhas: Vec<String>) -> Result<Vec<String>> {
    if falhas.is_empty() {
        return Ok(done);
    }
    let feitos = if done.is_empty() {
        String::new()
    } else {
        format!(" Configurado em: {}.", done.join("; "))
    };
    anyhow::bail!(
        "O bloqueador de anúncios não ficou configurado em {} navegador(es): {}.{feitos} O DNS continua aplicado.",
        falhas.len(),
        falhas.join(" | ")
    )
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

    /// Firefox onde o processo elevado pode gravar `policies.json`: só em `Program Files`.
    /// Uma instalação por usuário (`%LOCALAPPDATA%`) fica fora: o caminho é de uma conta comum
    /// e uma junção ali faria o administrador gravar ou apagar arquivos alheios (ADR-030).
    fn find_firefox() -> Option<PathBuf> {
        ["ProgramFiles", "ProgramFiles(x86)"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|d| PathBuf::from(d).join(r"Mozilla Firefox\firefox.exe"))
            .find(|p| p.is_file())
    }

    pub fn install_all() -> Result<Vec<String>> {
        let mut done = Vec::new();
        let mut falhas: Vec<String> = Vec::new();
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
            let Some((_, key, data)) = chromium_policies().into_iter().find(|p| p.0 == name) else {
                continue;
            };
            // E1: um `9101` de outra política não é sobrescrito
            if install_decision(name, read_policy_value(key).as_deref())
                == InstallDecision::SkipForeign
            {
                falhas.push(format!(
                    "{name} (o valor 9101 já é usado por outra política; não o sobrescrevi)"
                ));
                continue;
            }
            let r = reg_add(key, &data).and_then(|()| {
                // só diz que instalou o que a leitura de volta confirma
                if read_policy_value(key).is_some_and(|d| policy_value_covers(name, &d)) {
                    Ok(())
                } else {
                    anyhow::bail!("a política não ficou gravada como esperado")
                }
            });
            match r {
                Ok(()) => done.push(format!(
                    "{name}: uBlock Origin Lite instalado ao reabrir o navegador"
                )),
                Err(e) => falhas.push(format!("{name} ({e:#})")),
            }
        }
        if let Some(exe) = find_firefox() {
            // E2: o Firefox também entra na lista de falhas, sem derrubar o que já foi feito
            let r = (|| -> Result<()> {
                let dir = exe.parent().context("pasta do Firefox")?;
                // sem seguir link nem junção em `distribution` nem em `policies.json` (ADR-030)
                let owner = crate::safefs::caller_owner();
                let current =
                    crate::safefs::read_file(dir, &["distribution"], "policies.json", &owner)?
                        .map(|b| String::from_utf8_lossy(&b).into_owned());
                crate::safefs::write_file(
                    dir,
                    &["distribution"],
                    "policies.json",
                    merge_firefox_policies(current.as_deref())?.as_bytes(),
                )
                .context("gravando policies.json do Firefox")
            })();
            match r {
                Ok(()) => {
                    done.push("Firefox: uBlock Origin instalado ao reabrir o navegador".into())
                }
                Err(e) => falhas.push(format!("Firefox ({e:#})")),
            }
        }
        summarize_install(done, falhas)
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
            // coberto só se a política gravada é a certa DESTA loja (o valor antigo do Edge,
            // com o ID da loja do Chrome, não instala nada e conta como «sem bloqueador»)
            let has = read_policy_value(key).is_some_and(|d| policy_value_covers(name, &d));
            if !has {
                out.push(name.to_string());
            }
        }
        if let Some(exe) = find_firefox() {
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
        for (name, key, _) in chromium_policies() {
            // só apaga o valor se o dado é do GuiaLar (o atual ou o antigo errado do Edge);
            // um `9101` de terceiros fica intacto
            let existing = read_policy_value(key);
            if cleanup_action(name, existing.as_deref()) != PolicyCleanup::Remove {
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
        if let Some(exe) = find_firefox() {
            let dir = exe.parent().context("pasta do Firefox")?;
            let owner = crate::safefs::caller_owner();
            let current =
                crate::safefs::read_file(dir, &["distribution"], "policies.json", &owner)?;
            if let Some(current) = current.map(|b| String::from_utf8_lossy(&b).into_owned()) {
                if let Some(novo) = remove_firefox_policy(&current)? {
                    if is_empty_policies(&novo) {
                        // Criado só por nós: não deixa arquivo vazio para trás.
                        crate::safefs::remove_file(dir, &["distribution"], "policies.json", &owner)
                            .context("removendo policies.json vazio")?;
                    } else {
                        crate::safefs::write_file(
                            dir,
                            &["distribution"],
                            "policies.json",
                            novo.as_bytes(),
                        )
                        .context("gravando policies.json do Firefox")?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Dado do valor `9101` da chave, se existir.
    fn read_policy_value(key: &str) -> Option<String> {
        let out = hidden(Command::new("reg").args(["query", key, "/v", POLICY_VALUE_NAME]))
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| parse_reg_sz(&String::from_utf8_lossy(&out.stdout), POLICY_VALUE_NAME))
            .flatten()
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

    fn politica(browser: &str) -> (&'static str, String) {
        chromium_policies()
            .into_iter()
            .find(|p| p.0 == browser)
            .map(|(_, k, d)| (k, d))
            .unwrap_or_else(|| panic!("sem política para {browser}"))
    }

    #[test]
    fn chromium_tres_navegadores() {
        let p = chromium_policies();
        assert_eq!(p.len(), 3);
        assert!(p
            .iter()
            .all(|(_, k, d)| k.starts_with(r"HKLM\SOFTWARE\Policies") && d.contains(";https://")));
    }

    #[test]
    fn chrome_recebe_o_ubo_lite_da_chrome_web_store() {
        let (k, d) = politica("Google Chrome");
        assert!(k.contains(r"Policies\Google\Chrome"));
        assert_eq!(
            d,
            "ddkjiahejlhfcafbddmgiahcphecmpfh;https://clients2.google.com/service/update2/crx"
        );
    }

    #[test]
    fn brave_recebe_o_ubo_lite_da_chrome_web_store() {
        let (k, d) = politica("Brave");
        assert!(k.contains(r"Policies\BraveSoftware\Brave"));
        assert_eq!(
            d,
            "ddkjiahejlhfcafbddmgiahcphecmpfh;https://clients2.google.com/service/update2/crx"
        );
    }

    #[test]
    fn edge_recebe_o_ubo_lite_da_loja_do_edge_e_nunca_o_id_da_chrome_web_store() {
        let (k, d) = politica("Microsoft Edge");
        assert!(k.contains(r"Policies\Microsoft\Edge"));
        assert_eq!(
            d,
            "cimighlppcgcoapaliogpjjdehbnofhn;https://edge.microsoft.com/extensionwebstorebase/v1/crx"
        );
        assert!(
            !d.contains(UBOL_CWS_ID),
            "o serviço do Edge não conhece esse ID"
        );
        assert!(!d.contains("clients2.google.com"));
    }

    #[test]
    fn cobertura_so_vale_para_a_politica_certa_da_loja() {
        let (_, edge) = politica("Microsoft Edge");
        let (_, chrome) = politica("Google Chrome");
        let antigo = format!("{UBOL_CWS_ID};{EDGE_UPDATE_URL}");
        assert!(policy_value_covers("Microsoft Edge", &edge));
        assert!(policy_value_covers(
            "Google Chrome",
            &format!("  {chrome} ")
        ));
        // o valor antigo do Edge e o dado de outra loja NÃO contam como bloqueador instalado
        assert!(!policy_value_covers("Microsoft Edge", &antigo));
        assert!(!policy_value_covers("Microsoft Edge", &chrome));
        assert!(!policy_value_covers("Google Chrome", &edge));
        assert!(!policy_value_covers("Brave", &edge));
        assert!(!policy_value_covers("Chromium", &chrome));
    }

    #[test]
    fn limpeza_remove_o_valor_antigo_errado_do_edge_sem_tocar_em_terceiros() {
        let (_, edge) = politica("Microsoft Edge");
        let (_, chrome) = politica("Google Chrome");
        let antigo = format!("{UBOL_CWS_ID};{EDGE_UPDATE_URL}");
        // o valor antigo errado e o atual saem
        assert_eq!(
            cleanup_action("Microsoft Edge", Some(&antigo)),
            PolicyCleanup::Remove
        );
        assert_eq!(
            cleanup_action("Microsoft Edge", Some(&edge)),
            PolicyCleanup::Remove
        );
        assert_eq!(
            cleanup_action("Google Chrome", Some(&chrome)),
            PolicyCleanup::Remove
        );
        // um `9101` de outro programa/administrador fica
        assert_eq!(
            cleanup_action("Microsoft Edge", Some("abcdefgh;https://exemplo.test/crx")),
            PolicyCleanup::Keep
        );
        // o «antigo» só vale para o Edge: no Chrome o mesmo dado não é nosso
        assert_eq!(
            cleanup_action("Google Chrome", Some(&antigo)),
            PolicyCleanup::Keep
        );
        assert_eq!(
            cleanup_action("Microsoft Edge", None),
            PolicyCleanup::Absent
        );
    }

    #[test]
    fn install_nao_sobrescreve_um_9101_de_terceiros() {
        let (_, edge) = politica("Microsoft Edge");
        let antigo = format!("{UBOL_CWS_ID};{EDGE_UPDATE_URL}");
        // sem valor, valor nosso (o atual) e o valor antigo errado do Edge: grava
        assert_eq!(
            install_decision("Microsoft Edge", None),
            InstallDecision::Write
        );
        assert_eq!(
            install_decision("Microsoft Edge", Some(&edge)),
            InstallDecision::Write
        );
        assert_eq!(
            install_decision("Microsoft Edge", Some(&antigo)),
            InstallDecision::Write
        );
        // 9101 de outra política: pula, sem sobrescrever
        assert_eq!(
            install_decision("Microsoft Edge", Some("abcdefgh;https://exemplo.test/crx")),
            InstallDecision::SkipForeign
        );
        assert_eq!(
            install_decision("Google Chrome", Some(&antigo)),
            InstallDecision::SkipForeign,
            "o valor antigo do Edge não é nosso no Chrome"
        );
    }

    #[test]
    fn falha_num_navegador_nao_derruba_os_outros_e_diz_que_o_dns_continua() {
        let ok = vec!["Brave: uBlock Origin Lite instalado".to_string()];
        assert_eq!(summarize_install(ok.clone(), vec![]).unwrap(), ok);
        let e = summarize_install(
            ok,
            vec![
                "Microsoft Edge (releitura falhou)".into(),
                "Firefox (sem acesso)".into(),
            ],
        )
        .unwrap_err();
        let m = format!("{e:#}");
        assert!(
            m.contains("2 navegador(es)") && m.contains("Microsoft Edge") && m.contains("Firefox"),
            "{m}"
        );
        assert!(m.contains("Configurado em: Brave"), "{m}");
        assert!(m.contains("O DNS continua aplicado"), "{m}");
        // só falhas
        let m = format!(
            "{:#}",
            summarize_install(vec![], vec!["Edge (x)".into()]).unwrap_err()
        );
        assert!(
            !m.contains("Configurado em") && m.contains("O DNS continua aplicado"),
            "{m}"
        );
    }

    #[test]
    fn le_o_dado_da_saida_do_reg_query() {
        let saida = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Policies\\Microsoft\\Edge\\ExtensionInstallForcelist\r\n    9101    REG_SZ    cimighlppcgcoapaliogpjjdehbnofhn;https://edge.microsoft.com/extensionwebstorebase/v1/crx\r\n\r\n";
        assert_eq!(
            parse_reg_sz(saida, "9101").as_deref(),
            Some("cimighlppcgcoapaliogpjjdehbnofhn;https://edge.microsoft.com/extensionwebstorebase/v1/crx")
        );
        assert_eq!(parse_reg_sz(saida, "1"), None);
        assert_eq!(parse_reg_sz("", "9101"), None);
        // `91011` não é o valor `9101`
        assert_eq!(parse_reg_sz("    91011    REG_SZ    x\r\n", "9101"), None);
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

    #[test]
    fn policies_vazio_e_detectado() {
        assert!(is_empty_policies(r#"{"policies":{}}"#));
        assert!(!is_empty_policies(
            r#"{"policies":{"DisableTelemetry":true}}"#
        ));
        assert!(!is_empty_policies("nao json"));
        let so_nosso = merge_firefox_policies(None).unwrap();
        let novo = remove_firefox_policy(&so_nosso).unwrap().unwrap();
        assert!(is_empty_policies(&novo));
    }

    #[test]
    fn mescla_e_idempotente() {
        let uma = merge_firefox_policies(None).unwrap();
        let duas = merge_firefox_policies(Some(&uma)).unwrap();
        assert_eq!(uma, duas);
    }

    #[test]
    fn mescla_com_arquivo_vazio_ou_so_espacos_cria_do_zero() {
        let esperado = merge_firefox_policies(None).unwrap();
        assert_eq!(merge_firefox_policies(Some("")).unwrap(), esperado);
        assert_eq!(merge_firefox_policies(Some("  \n ")).unwrap(), esperado);
    }

    #[test]
    fn mescla_preserva_outras_politicas_e_extensoes() {
        let atual = r#"{"policies":{"Homepage":{"URL":"https://a.b"},"ExtensionSettings":{"*":{"installation_mode":"allowed"},"x@y":{"installation_mode":"blocked"}}},"outro":1}"#;
        let v: Value = serde_json::from_str(&merge_firefox_policies(Some(atual)).unwrap()).unwrap();
        assert_eq!(v["outro"], 1);
        assert_eq!(v["policies"]["Homepage"]["URL"], "https://a.b");
        assert_eq!(
            v["policies"]["ExtensionSettings"]["*"]["installation_mode"],
            "allowed"
        );
        assert_eq!(
            v["policies"]["ExtensionSettings"]["x@y"]["installation_mode"],
            "blocked"
        );
        assert_eq!(
            v["policies"]["ExtensionSettings"][FIREFOX_UBO_ID]["install_url"],
            FIREFOX_UBO_XPI
        );
    }

    #[test]
    fn mescla_sobrescreve_so_a_entrada_do_guialar() {
        let atual = format!(
            r#"{{"policies":{{"ExtensionSettings":{{"{FIREFOX_UBO_ID}":{{"installation_mode":"blocked"}}}}}}}}"#
        );
        let v: Value =
            serde_json::from_str(&merge_firefox_policies(Some(&atual)).unwrap()).unwrap();
        assert_eq!(
            v["policies"]["ExtensionSettings"][FIREFOX_UBO_ID]["installation_mode"],
            "force_installed"
        );
    }

    #[test]
    fn mescla_rejeita_formas_inesperadas_sem_perder_o_arquivo() {
        for ruim in [
            "[]",
            r#"{"policies":[]}"#,
            r#"{"policies":{"ExtensionSettings":"x"}}"#,
            r#"{"policies":{"ExtensionSettings":[1]}}"#,
        ] {
            assert!(merge_firefox_policies(Some(ruim)).is_err(), "{ruim}");
        }
    }

    #[test]
    fn mescla_e_remocao_se_desfazem() {
        let atual = r#"{"policies":{"DisableTelemetry":true}}"#;
        let mesclado = merge_firefox_policies(Some(atual)).unwrap();
        let removido = remove_firefox_policy(&mesclado).unwrap().unwrap();
        let v: Value = serde_json::from_str(&removido).unwrap();
        assert_eq!(v, serde_json::from_str::<Value>(atual).unwrap());
    }
}

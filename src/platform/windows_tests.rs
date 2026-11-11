//! Testes do backend Windows com um PowerShell falso (ADR-029, camada 2). Só compilam e
//! rodam no Windows (VM win11 ou CI): nada aqui toca o DNS real nem pede UAC.

use super::*;
use crate::platform::runner::{Cmd, CommandRunner, Outcome};
use crate::testenv::Sandbox;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;

const FAMILY: &str = "'1.1.1.3','1.0.0.3','2606:4700:4700::1113','2606:4700:4700::1003'";

/// Inverso de `encode_powershell`: base64 de UTF-16LE.
fn decode_ps(b64: &str) -> String {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut bytes = Vec::new();
    for ch in b64.trim_end_matches('=').as_bytes().chunks(4) {
        let v: Vec<u8> = ch.iter().map(|c| val(*c)).collect();
        bytes.push((v[0] << 2) | (v[1] >> 4));
        if v.len() > 2 {
            bytes.push((v[1] << 4) | (v[2] >> 2));
        }
        if v.len() > 3 {
            bytes.push((v[2] << 6) | v[3]);
        }
    }
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16(&units).unwrap()
}

/// PowerShell/ipconfig falsos: interfaces com DNS por índice.
struct PsFake {
    ifaces: Vec<(String, u32)>,
    dns: RefCell<BTreeMap<u32, Vec<String>>>,
    calls: RefCell<Vec<String>>,
    fail_on: RefCell<Option<String>>,
}

impl PsFake {
    fn new(ifaces: &[(&str, u32, &[&str])]) -> Self {
        Self {
            ifaces: ifaces.iter().map(|(a, i, _)| (a.to_string(), *i)).collect(),
            dns: RefCell::new(
                ifaces
                    .iter()
                    .map(|(_, i, d)| (*i, d.iter().map(|s| s.to_string()).collect()))
                    .collect(),
            ),
            calls: RefCell::new(Vec::new()),
            fail_on: RefCell::new(None),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn dns_of(&self, index: u32) -> Vec<String> {
        self.dns.borrow()[&index].clone()
    }

    fn all_dns(&self) -> BTreeMap<u32, Vec<String>> {
        self.dns.borrow().clone()
    }

    fn index_of_alias(&self, alias: &str) -> u32 {
        self.ifaces.iter().find(|(a, _)| a == alias).unwrap().1
    }

    fn stmt(&self, st: &str) -> String {
        let st = st.trim();
        let index = |s: &str| -> u32 {
            if let Some(rest) = s.split("-InterfaceIndex ").nth(1) {
                rest.split(|c: char| !c.is_ascii_digit())
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            } else {
                let alias = s
                    .split("-InterfaceAlias '")
                    .nth(1)
                    .unwrap()
                    .split('\'')
                    .next()
                    .unwrap();
                self.index_of_alias(alias)
            }
        };
        if st.starts_with("Get-NetIPInterface") {
            return self
                .ifaces
                .iter()
                .map(|(a, i)| format!("{a}|{i}\n"))
                .collect();
        }
        if let Some(rest) = st.strip_prefix("(Get-DnsClientServerAddress") {
            let idx = index(rest);
            let all = self.dns_of(idx);
            let v: Vec<String> = if rest.contains("-AddressFamily IPv4") {
                all.into_iter().filter(|s| !s.contains(':')).collect()
            } else {
                all
            };
            return v.join(",");
        }
        if let Some(rest) = st.strip_prefix("Set-DnsClientServerAddress") {
            let idx = index(rest);
            if rest.contains("-ResetServerAddresses") {
                self.dns.borrow_mut().insert(idx, Vec::new());
            } else {
                let list = rest.split("@(").nth(1).unwrap().trim_end_matches(')');
                let v = list
                    .split(',')
                    .map(|t| t.trim_matches('\'').to_string())
                    .collect();
                self.dns.borrow_mut().insert(idx, v);
            }
        }
        String::new()
    }
}

impl CommandRunner for PsFake {
    fn run(&self, cmd: &Cmd) -> io::Result<Outcome> {
        assert!(
            cmd.no_window,
            "powershell/ipconfig precisam de CREATE_NO_WINDOW"
        );
        if cmd.program == "ipconfig" {
            self.calls
                .borrow_mut()
                .push(format!("ipconfig {}", cmd.args[0].to_string_lossy()));
            return Ok(Outcome {
                code: Some(0),
                ..Outcome::default()
            });
        }
        assert_eq!(cmd.program, "powershell");
        let script = decode_ps(&cmd.args.last().unwrap().to_string_lossy());
        let script = script
            .strip_prefix(PS_PREFIX)
            .expect("prefixo UTF-8/Stop")
            .to_string();
        self.calls.borrow_mut().push(script.clone());
        if let Some(f) = self.fail_on.borrow().as_deref() {
            if script.contains(f) {
                return Ok(Outcome {
                    code: Some(1),
                    stderr: b"falhou".to_vec(),
                    ..Outcome::default()
                });
            }
        }
        let out: String = script.split(';').map(|s| self.stmt(s)).collect();
        Ok(Outcome {
            code: Some(0),
            stdout: out.into_bytes(),
            ..Outcome::default()
        })
    }
}

fn two_ifaces() -> PsFake {
    PsFake::new(&[
        ("Ethernet", 5, &[]),
        ("Wi-Fi", 7, &["192.168.0.1", "8.8.8.8"]),
    ])
}

#[test]
fn powershell_vai_em_base64_sem_janela_e_com_erro_fatal() {
    let c = powershell("Get-Date");
    assert_eq!(c.program, "powershell");
    let a: Vec<String> = c
        .args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        a[..5],
        [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand"
        ]
    );
    assert_eq!(decode_ps(&a[5]), format!("{PS_PREFIX}Get-Date"));
    assert!(c.no_window);
    assert!(PS_PREFIX.contains("$ErrorActionPreference='Stop'"));
}

#[test]
fn apply_usa_os_scripts_de_producao_na_ordem() {
    let s = Sandbox::new("win-apply");
    let r = two_ifaces();
    apply_core_with(&r).unwrap();
    let set = |i: u32| {
        format!("Set-DnsClientServerAddress -InterfaceIndex {i} -ServerAddresses @({FAMILY})")
    };
    let ver = |i: u32| {
        format!("(Get-DnsClientServerAddress -InterfaceIndex {i} -AddressFamily IPv4).ServerAddresses -join ','")
    };
    let calls = r.calls();
    assert!(
        calls[0].starts_with("Get-NetIPInterface -AddressFamily IPv4 -ConnectionState Connected")
    );
    assert_eq!(
        calls[1..],
        [
            ver(5),
            set(5),
            ver(7),
            set(7),
            "ipconfig /flushdns".to_string()
        ]
    );
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert_eq!(m.backend, BackendKind::WindowsPowerShell);
    assert_eq!(m.connections[0].id, "Ethernet");
    assert!(m.connections[0].was_dhcp_ipv4);
    assert_eq!(m.connections[1].original_ipv4_dns.len(), 2);
    assert!(s.manifest_path().exists());
}

#[test]
fn apply_depois_restaurar_devolve_o_dns_original() {
    let _s = Sandbox::new("win-roundtrip");
    let r = two_ifaces();
    let original = r.all_dns();
    apply_core_with(&r).unwrap();
    assert_ne!(r.all_dns(), original);
    let m = DnsStateManifest::load().unwrap().unwrap();
    r.calls.borrow_mut().clear();
    restore_connections(&r, &m).unwrap();
    assert_eq!(r.all_dns(), original);
    assert_eq!(
        r.calls(),
        [
            "Set-DnsClientServerAddress -InterfaceAlias 'Ethernet' -ResetServerAddresses",
            "Set-DnsClientServerAddress -InterfaceAlias 'Wi-Fi' -ResetServerAddresses; Set-DnsClientServerAddress -InterfaceAlias 'Wi-Fi' -ServerAddresses @('192.168.0.1','8.8.8.8')",
        ]
    );
}

#[test]
fn falha_do_powershell_nao_grava_manifesto() {
    let s = Sandbox::new("win-fail");
    let r = two_ifaces();
    *r.fail_on.borrow_mut() = Some("-ServerAddresses @(".into());
    let e = apply_core_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("PowerShell falhou"));
    assert!(!s.manifest_path().exists());
}

#[test]
fn sem_interface_ativa_nao_aplica_nada() {
    let s = Sandbox::new("win-noif");
    let r = PsFake::new(&[]);
    let e = apply_core_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("Nenhuma interface"));
    assert!(!s.manifest_path().exists());
    assert_eq!(r.calls().len(), 1);
}

#[test]
fn restaurar_segue_nas_outras_interfaces_e_relata_a_que_falhou() {
    let _s = Sandbox::new("win-restore-partial");
    let r = two_ifaces();
    let mut m = DnsStateManifest::new(BackendKind::WindowsPowerShell);
    for (id, dhcp) in [("Adaptador ' ruim", true), ("Wi-Fi", true)] {
        m.connections.push(ConnectionBackup {
            id: id.into(),
            original_ipv4_dns: Vec::new(),
            original_ipv6_dns: Vec::new(),
            was_dhcp_ipv4: dhcp,
            was_dhcp_ipv6: true,
        });
    }
    r.dns.borrow_mut().insert(7, vec!["1.1.1.3".into()]);
    let e = restore_connections(&r, &m).unwrap_err();
    assert!(format!("{e:#}").contains("alias inválido"), "{e:#}");
    assert!(
        r.dns_of(7).is_empty(),
        "a interface boa foi restaurada mesmo assim"
    );
}

#[test]
fn trocar_de_modo_usa_o_alias_e_o_dns_do_novo_modo() {
    let _s = Sandbox::new("win-switch");
    let r = two_ifaces();
    apply_core_with(&r).unwrap();
    let m = DnsStateManifest::load().unwrap().unwrap();
    r.calls.borrow_mut().clear();
    set_servers_on_connections(&r, &m, crate::config::FilterMode::AdsOnly).unwrap();
    let ads = crate::config::FilterMode::AdsOnly;
    let lista = format!(
        "'{}','{}','{}','{}'",
        ads.ipv4()[0],
        ads.ipv4()[1],
        ads.ipv6()[0],
        ads.ipv6()[1]
    );
    assert_eq!(
        r.calls(),
        [
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias 'Ethernet' -ServerAddresses @({lista})"
            ),
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias 'Wi-Fi' -ServerAddresses @({lista})"
            ),
        ]
    );
}

#[test]
fn reversao_de_emergencia_zera_todas_as_interfaces_por_indice() {
    let _s = Sandbox::new("win-reset");
    let r = two_ifaces();
    reset_all_interfaces(&r).unwrap();
    assert!(r.calls()[1..]
        .iter()
        .all(|c| c.ends_with("-ResetServerAddresses")));
    assert_eq!(
        r.calls()[1],
        "Set-DnsClientServerAddress -InterfaceIndex 5 -ResetServerAddresses"
    );
    assert!(r.all_dns().values().all(|v| v.is_empty()));
}

#[test]
fn dns_configurado_junta_ipv4_e_ipv6_sem_repetir() {
    let r = PsFake::new(&[
        ("Ethernet", 5, &["1.1.1.3", "2606:4700:4700::1113"]),
        ("Wi-Fi", 7, &["1.1.1.3"]),
    ]);
    let ips: Vec<String> = configured_dns_with(&r)
        .iter()
        .map(|i| i.to_string())
        .collect();
    assert_eq!(ips, ["1.1.1.3", "2606:4700:4700::1113"]);
}

// LIMITE (ADR-029, «Limites»): `run_elevated_hidden` (UAC) fica fora da costura, então aqui só
// se prova a tradução do resultado do UAC. Que um UAC negado deixa manifesto e DNS intactos no
// Windows real é coberto pelo E2E do gl-qa (scripts/e2e.ps1) e pela checagem manual do release
// candidate. No Linux o mesmo critério passa pela costura (linux_tests.rs).
#[test]
fn uac_negado_ou_falho_nao_conta_como_feito() {
    let negado =
        elevation_result(Err("a permissão de administrador foi negada".into())).unwrap_err();
    let msg = format!("{negado:#}");
    assert!(crate::ui::theme::humanize_error(&msg)
        .0
        .contains("Nada foi alterado"));
    let falhou = elevation_result(Ok(1)).unwrap_err();
    assert!(format!("{falhou:#}").contains("código 1"));
    assert!(elevation_result(Ok(0)).unwrap());
    let timeout = elevation_result(Err(
        "a operação com permissão de administrador demorou demais".into(),
    ));
    assert!(timeout.is_err());
}

#[test]
fn limpar_cache_dns_chama_ipconfig_sem_janela() {
    let r = two_ifaces();
    flush_dns(&r);
    assert_eq!(r.calls(), ["ipconfig /flushdns"]);
}

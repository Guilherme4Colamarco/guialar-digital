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

/// PowerShell/ipconfig falsos. Cada interface tem o DNS **estático** (`NameServer` do registro,
/// v4 e v6) e o do DHCP; o DNS efetivo é o estático da família, se houver, senão o do DHCP.
struct PsFake {
    ifaces: Vec<(String, u32)>,
    reg4: RefCell<BTreeMap<u32, Vec<String>>>,
    reg6: RefCell<BTreeMap<u32, Vec<String>>>,
    dhcp: BTreeMap<u32, Vec<String>>,
    calls: RefCell<Vec<String>>,
    fail_on: RefCell<Option<String>>,
    /// uma segunda parte de script que também falha
    fail_also: RefCell<Option<String>>,
    /// interfaces cujo `InterfaceGuid` volta num formato inválido
    bad_guid: RefCell<Vec<u32>>,
}

fn guid_of(index: u32) -> String {
    format!("{{00000000-0000-0000-0000-{index:012}}}")
}

fn index_of_guid(text: &str) -> u32 {
    let i = text.find("{00000000-0000-0000-0000-").unwrap() + 25;
    text[i..i + 12].parse().unwrap()
}

impl PsFake {
    /// `dns` = DNS configurado à mão (estático) na interface.
    fn new(ifaces: &[(&str, u32, &[&str])]) -> Self {
        let f = Self::empty(ifaces);
        for (_, i, d) in ifaces {
            let all: Vec<String> = d.iter().map(|s| s.to_string()).collect();
            f.reg4.borrow_mut().insert(
                *i,
                all.iter().filter(|s| !s.contains(':')).cloned().collect(),
            );
            f.reg6.borrow_mut().insert(
                *i,
                all.iter().filter(|s| s.contains(':')).cloned().collect(),
            );
        }
        f
    }

    /// Interfaces por DHCP: sem DNS estático; `dns` é o que o roteador entrega.
    fn new_dhcp(ifaces: &[(&str, u32, &[&str])]) -> Self {
        let mut f = Self::empty(ifaces);
        f.dhcp = ifaces
            .iter()
            .map(|(_, i, d)| (*i, d.iter().map(|s| s.to_string()).collect()))
            .collect();
        f
    }

    fn empty(ifaces: &[(&str, u32, &[&str])]) -> Self {
        Self {
            ifaces: ifaces.iter().map(|(a, i, _)| (a.to_string(), *i)).collect(),
            reg4: RefCell::new(ifaces.iter().map(|(_, i, _)| (*i, Vec::new())).collect()),
            reg6: RefCell::new(ifaces.iter().map(|(_, i, _)| (*i, Vec::new())).collect()),
            dhcp: BTreeMap::new(),
            calls: RefCell::new(Vec::new()),
            fail_on: RefCell::new(None),
            fail_also: RefCell::new(None),
            bad_guid: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    /// DNS efetivo (o que `Get-DnsClientServerAddress` devolve).
    fn dns_of(&self, index: u32) -> Vec<String> {
        let (r4, r6) = (
            self.reg4.borrow()[&index].clone(),
            self.reg6.borrow()[&index].clone(),
        );
        let dhcp = self.dhcp.get(&index).cloned().unwrap_or_default();
        let pick = |reg: Vec<String>, v6: bool| -> Vec<String> {
            if reg.is_empty() {
                dhcp.iter()
                    .filter(|s| s.contains(':') == v6)
                    .cloned()
                    .collect()
            } else {
                reg
            }
        };
        let mut out = pick(r4, false);
        out.extend(pick(r6, true));
        out
    }

    /// `NameServer` estático (v4, v6) da interface.
    fn registry_of(&self, index: u32) -> (Vec<String>, Vec<String>) {
        (
            self.reg4.borrow()[&index].clone(),
            self.reg6.borrow()[&index].clone(),
        )
    }

    fn all_dns(&self) -> BTreeMap<u32, Vec<String>> {
        self.ifaces
            .iter()
            .map(|(_, i)| (*i, self.dns_of(*i)))
            .collect()
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
        if let Some(rest) = st.strip_prefix("(Get-NetAdapter -InterfaceIndex ") {
            let idx: u32 = rest.split(')').next().unwrap().parse().unwrap();
            if self.bad_guid.borrow().contains(&idx) {
                return "sem-guid".into();
            }
            return guid_of(idx);
        }
        if st.starts_with("(Get-ItemProperty -LiteralPath 'HKLM:") {
            let idx = index_of_guid(st);
            let reg = if st.contains("\\Tcpip6\\") {
                &self.reg6
            } else {
                &self.reg4
            };
            return reg.borrow()[&idx].join(",");
        }
        if let Some(rest) = st.strip_prefix("(Get-DnsClientServerAddress") {
            let idx = index(rest);
            return self.dns_of(idx).join(",");
        }
        if let Some(rest) = st.strip_prefix("Set-DnsClientServerAddress") {
            let idx = index(rest);
            if rest.contains("-ResetServerAddresses") {
                self.reg4.borrow_mut().insert(idx, Vec::new());
                self.reg6.borrow_mut().insert(idx, Vec::new());
            } else {
                let list = rest.split("@(").nth(1).unwrap().trim_end_matches(')');
                let v: Vec<String> = list
                    .split(',')
                    .map(|t| t.trim_matches('\'').to_string())
                    .collect();
                // como o cmdlet: troca só a família presente na lista
                let v4: Vec<String> = v.iter().filter(|s| !s.contains(':')).cloned().collect();
                let v6: Vec<String> = v.iter().filter(|s| s.contains(':')).cloned().collect();
                if !v4.is_empty() {
                    self.reg4.borrow_mut().insert(idx, v4);
                }
                if !v6.is_empty() {
                    self.reg6.borrow_mut().insert(idx, v6);
                }
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
        let falha =
            |f: &RefCell<Option<String>>| f.borrow().as_deref().is_some_and(|f| script.contains(f));
        if falha(&self.fail_on) || falha(&self.fail_also) {
            return Ok(Outcome {
                code: Some(1),
                stderr: b"falhou".to_vec(),
                ..Outcome::default()
            });
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
    // primeiro LÊ todas as interfaces (GUID, NameServer v4 e v6); só depois troca o DNS
    let reg = |i: u32, fam: &str| {
        format!(
            "(Get-ItemProperty -LiteralPath 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\{fam}\\Parameters\\Interfaces\\{}' -Name NameServer -ErrorAction SilentlyContinue).NameServer",
            guid_of(i)
        )
    };
    let guid = |i: u32| format!("(Get-NetAdapter -InterfaceIndex {i}).InterfaceGuid");
    let calls = r.calls();
    assert!(
        calls[0].starts_with("Get-NetIPInterface -AddressFamily IPv4 -ConnectionState Connected")
    );
    assert_eq!(
        calls[1..],
        [
            guid(5),
            reg(5, "Tcpip"),
            reg(5, "Tcpip6"),
            guid(7),
            reg(7, "Tcpip"),
            reg(7, "Tcpip6"),
            set(5),
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
    r.reg4.borrow_mut().insert(7, vec!["1.1.1.3".into()]);
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

// ------------------------------------- DHCP × estático pelo registro (NameServer)

#[test]
fn interface_dhcp_nao_guarda_o_dns_do_roteador_como_estatico_e_volta_com_namesever_vazio() {
    let _s = Sandbox::new("win-dhcp");
    // o roteador entrega 192.168.0.1 por DHCP; `NameServer` do registro está vazio
    let r = PsFake::new_dhcp(&[("Wi-Fi", 7, &["192.168.0.1"])]);
    let original = r.all_dns();
    assert_eq!(r.registry_of(7), (vec![], vec![]));
    apply_core_with(&r).unwrap();
    // depois do apply o filtro está no registro
    assert_eq!(r.registry_of(7).0, ["1.1.1.3", "1.0.0.3"]);
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert!(m.connections[0].was_dhcp_ipv4 && m.connections[0].was_dhcp_ipv6);
    assert!(
        m.connections[0].original_ipv4_dns.is_empty(),
        "o DNS do DHCP não vira «fixo»: {:?}",
        m.connections[0].original_ipv4_dns
    );
    r.calls.borrow_mut().clear();
    restore_connections(&r, &m).unwrap();
    assert_eq!(
        r.calls(),
        ["Set-DnsClientServerAddress -InterfaceAlias 'Wi-Fi' -ResetServerAddresses"]
    );
    // NameServer vazio de novo (DHCP), e o DNS efetivo volta a ser o do roteador
    assert_eq!(r.registry_of(7), (vec![], vec![]));
    assert_eq!(r.all_dns(), original);
}

#[test]
fn interface_com_dns_estatico_v4_e_v6_volta_com_os_dois() {
    let _s = Sandbox::new("win-static46");
    let r = PsFake::new(&[("Ethernet", 5, &["192.168.0.1", "8.8.8.8", "2001:db8::1"])]);
    let (r4, r6) = r.registry_of(5);
    apply_core_with(&r).unwrap();
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert!(!m.connections[0].was_dhcp_ipv4 && !m.connections[0].was_dhcp_ipv6);
    assert_eq!(m.connections[0].original_ipv6_dns.len(), 1);
    restore_connections(&r, &m).unwrap();
    assert_eq!(r.registry_of(5), (r4, r6));
}

#[test]
fn v4_por_dhcp_e_v6_estatico_voltam_cada_um_como_eram() {
    let _s = Sandbox::new("win-mix");
    let mut r = PsFake::new_dhcp(&[("Ethernet", 5, &["192.168.0.1"])]);
    r.reg6.borrow_mut().insert(5, vec!["2001:db8::1".into()]);
    r.dhcp.insert(5, vec!["192.168.0.1".into()]);
    apply_core_with(&r).unwrap();
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert!(m.connections[0].was_dhcp_ipv4 && !m.connections[0].was_dhcp_ipv6);
    restore_connections(&r, &m).unwrap();
    assert_eq!(r.registry_of(5), (vec![], vec!["2001:db8::1".to_string()]));
}

#[test]
fn dns_so_de_filtro_no_registro_vira_automatico() {
    // apply sobre rede já filtrada: o «original» nunca é o filtro do GuiaLar
    let _s = Sandbox::new("win-filtrado");
    let r = PsFake::new(&[("Ethernet", 5, &["1.1.1.3", "1.0.0.3"])]);
    apply_core_with(&r).unwrap();
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert!(m.connections[0].was_dhcp_ipv4 && m.connections[0].original_ipv4_dns.is_empty());
}

#[test]
fn guid_de_adaptador_invalido_nao_entra_no_caminho_do_registro() {
    assert!(is_adapter_guid("{0a1b2c3d-0000-4abc-9def-0123456789ab}"));
    for ruim in [
        "",
        "0a1b2c3d-0000-4abc-9def-0123456789ab",
        "{x}",
        "{0a1b2c3d-0000-4abc-9def-0123456789a'}",
        "{0a1b2c3d-0000-4abc-9def-0123456789ab}; calc",
    ] {
        assert!(!is_adapter_guid(ruim), "{ruim:?}");
    }
}

#[test]
fn name_server_aceita_virgula_espaco_e_ponto_e_virgula() {
    let v4: Vec<Ipv4Addr> = parse_name_server("8.8.8.8,8.8.4.4 1.1.1.1;9.9.9.9\r\n");
    assert_eq!(v4.len(), 4);
    let v6: Vec<std::net::Ipv6Addr> = parse_name_server("2001:db8::1, 2001:db8::2");
    assert_eq!(v6.len(), 2);
    assert!(parse_name_server::<Ipv4Addr>(" , ").is_empty());
}

// ------------------------------- inspecionar tudo antes de alterar qualquer interface

fn sets(r: &PsFake) -> Vec<String> {
    r.calls()
        .into_iter()
        .filter(|c| c.starts_with("Set-DnsClientServerAddress"))
        .collect()
}

#[test]
fn interface_sem_guid_e_pulada_com_aviso_e_as_outras_seguem() {
    let _s = Sandbox::new("win-sem-guid");
    // a SEGUNDA interface não tem GUID: antes, a primeira já estava alterada quando o apply falhava
    let r = two_ifaces();
    r.bad_guid.borrow_mut().push(7);
    let wifi_antes = r.dns_of(7);
    apply_core_with(&r).unwrap();
    let m = DnsStateManifest::load().unwrap().unwrap();
    assert_eq!(m.connections.len(), 1, "só a interface inspecionada");
    assert_eq!(m.connections[0].id, "Ethernet");
    assert_eq!(
        sets(&r).len(),
        1,
        "só a Ethernet foi alterada: {:?}",
        sets(&r)
    );
    assert_eq!(
        r.dns_of(7),
        wifi_antes,
        "a interface sem GUID ficou como era"
    );
}

#[test]
fn falha_ao_ler_uma_interface_nao_altera_nenhuma() {
    let _s = Sandbox::new("win-leitura-falha");
    let r = two_ifaces();
    let antes = r.all_dns();
    // a leitura do registro da SEGUNDA interface falha
    *r.fail_on.borrow_mut() = Some(guid_of(7));
    let e = apply_core_with(&r).unwrap_err();
    assert!(
        format!("{e:#}").contains("Falha ao consultar DNS atual"),
        "{e:#}"
    );
    assert!(
        sets(&r).is_empty(),
        "nenhum DNS foi trocado antes de o estado de todas ser lido: {:?}",
        sets(&r)
    );
    assert_eq!(r.all_dns(), antes);
    assert!(DnsStateManifest::load().unwrap().is_none(), "sem manifesto");
}

#[test]
fn todas_sem_guid_falha_sem_alterar_nada() {
    let _s = Sandbox::new("win-todas-sem-guid");
    let r = two_ifaces();
    r.bad_guid.borrow_mut().extend([5, 7]);
    let antes = r.all_dns();
    let e = apply_core_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("pôde ser inspecionada"), "{e:#}");
    assert!(sets(&r).is_empty());
    assert_eq!(r.all_dns(), antes);
}

// ------------------------------- falha num Set no meio do laço (B1 do 7ec97d7)

#[test]
fn falha_no_set_da_segunda_interface_desfaz_a_primeira_e_nao_deixa_manifesto() {
    let _s = Sandbox::new("win-set-falha");
    let r = two_ifaces();
    let antes = r.all_dns();
    *r.fail_on.borrow_mut() = Some("-InterfaceIndex 7 -ServerAddresses".into());
    let e = apply_core_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("foi desfeito"), "{e:#}");
    assert_eq!(r.all_dns(), antes, "a Ethernet voltou ao que era");
    assert!(DnsStateManifest::load().unwrap().is_none());
}

#[test]
fn falha_no_set_e_no_desfazer_deixa_o_manifesto_para_o_desativar() {
    let _s = Sandbox::new("win-set-falha-2");
    let r = two_ifaces();
    *r.fail_on.borrow_mut() = Some("-InterfaceIndex 7 -ServerAddresses".into());
    *r.fail_also.borrow_mut() = Some("-InterfaceAlias".into()); // o desfazer também falha
    let e = apply_core_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("não consegui desfazer"), "{e:#}");
    // a Ethernet já estava com o filtro; o registro do estado de antes existe
    let m = DnsStateManifest::load()
        .unwrap()
        .expect("manifesto gravado antes do laço");
    assert_eq!(m.connections.len(), 2);
}

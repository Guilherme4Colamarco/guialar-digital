//! Módulo de verificação unificado e smoke test de resolução DNS.

use std::net::{IpAddr, ToSocketAddrs};
use std::sync::mpsc;
use std::time::Duration;

use crate::config::{DnsStateManifest, FilterMode};

pub const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectionStatus {
    Protected,
    PartiallyConfigured(String),
    Unprotected,
}

/// Falta de permissão NÃO é arquivo corrompido (o arquivo pode ser de root).
pub fn describe_load_error(e: &anyhow::Error) -> String {
    let denied = e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::PermissionDenied)
    });
    if denied {
        format!("Sem permissão para ler o registro da proteção ({e:#}). Rode: sudo chown -R $USER ~/.config/guialar")
    } else {
        format!("Manifesto corrompido: {e}")
    }
}

/// Modo gravado no manifesto (se houver proteção).
pub fn active_mode() -> Option<FilterMode> {
    DnsStateManifest::load().ok().flatten().map(|m| m.mode)
}

/// Resultado da conferência do DNS configurado contra o conjunto do modo (sem rede).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DnsCheck {
    /// Todo DNS configurado (fora stubs locais) é do conjunto do modo.
    Ours,
    /// Há DNS configurado que não é do conjunto do modo.
    Foreign(Vec<IpAddr>),
    /// Não deu para ler nenhum DNS fora de stub local (ou nada): não dá para provar o DNS.
    Unknown,
}

/// Confere os DNS configurados (IPv4 e IPv6) com o conjunto do `mode`.
pub fn check_configured_dns(mode: FilterMode, configured: &[IpAddr]) -> DnsCheck {
    let real: Vec<IpAddr> = configured
        .iter()
        .copied()
        .filter(|ip| !crate::config::is_local_stub(ip))
        .collect();
    if real.is_empty() {
        return DnsCheck::Unknown;
    }
    let (v4, v6) = (mode.ipv4(), mode.ipv6());
    let foreign: Vec<IpAddr> = real
        .into_iter()
        .filter(|ip| match ip {
            IpAddr::V4(a) => !v4.contains(a),
            IpAddr::V6(a) => !v6.contains(a),
        })
        .collect();
    if foreign.is_empty() {
        DnsCheck::Ours
    } else {
        DnsCheck::Foreign(foreign)
    }
}

/// Frase do Parcial quando nenhum DNS pôde ser lido (a sonda sozinha não prova nada).
pub const MSG_DNS_UNREADABLE: &str = "Não consegui ler o DNS que este computador usa.";

/// Decide o estado com manifesto presente. `probe` só roda se o DNS é do modo (DNS de fora
/// e DNS ilegível já são Parcial sem gastar rede).
pub fn decide_status(
    mode: FilterMode,
    configured: &[IpAddr],
    probe: impl FnOnce() -> bool,
) -> ProtectionStatus {
    match check_configured_dns(mode, configured) {
        DnsCheck::Foreign(f) => {
            let lista: Vec<String> = f.iter().map(|i| i.to_string()).collect();
            return ProtectionStatus::PartiallyConfigured(format!(
                "Este computador usa outro DNS ({}), não o do GuiaLar. Desative a proteção e proteja de novo.",
                lista.join(", ")
            ));
        }
        DnsCheck::Unknown => {
            return ProtectionStatus::PartiallyConfigured(MSG_DNS_UNREADABLE.into());
        }
        DnsCheck::Ours => {}
    }
    if probe() {
        ProtectionStatus::Protected
    } else {
        ProtectionStatus::PartiallyConfigured(
            "DNS gravado, mas o filtro não respondeu no teste de rede".into(),
        )
    }
}

pub fn check_status() -> ProtectionStatus {
    let manifest_opt = match DnsStateManifest::load() {
        Ok(m) => m,
        Err(e) => return ProtectionStatus::PartiallyConfigured(describe_load_error(&e)),
    };

    let Some(manifest) = manifest_opt else {
        return ProtectionStatus::Unprotected;
    };
    let mode = manifest.mode;
    decide_status(mode, &crate::platform::configured_dns(), || {
        run_dns_smoke_test(mode)
    })
}

/// O nome de teste do `mode` foi bloqueado pelo filtro (resposta 0.0.0.0 / ::).
///
/// Perguntar o endereço do próprio servidor do filtro não prova nada: qualquer DNS
/// responde igual (uma VPN ou o roteador trocando o DNS passava por "protegido").
/// Só o filtro devolve "bloqueado" para um domínio de teste de malware/anúncio.
pub fn resolution_matches(addrs: &[IpAddr], _mode: FilterMode) -> bool {
    addrs.iter().any(|ip| ip.is_unspecified())
}

/// Smoke test com timeout: rede quebrada não pode travar a CLI.
fn run_dns_smoke_test(mode: FilterMode) -> bool {
    // Logo após `apply`/troca de modo o resolvedor leva um tempo para estabilizar.
    // Falhas rápidas são repetidas; falhas lentas (rede fora) não alongam a espera.
    let mut slow = 0;
    for attempt in 0..SMOKE_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(SMOKE_RETRY_DELAY);
        }
        let started = std::time::Instant::now();
        if smoke_once(mode) {
            return true;
        }
        if started.elapsed() >= Duration::from_secs(2) {
            slow += 1;
            if slow >= 2 {
                break;
            }
        }
    }
    false
}

/// WSANO_DATA: nome válido, sem registro (como o Windows relata uma resposta 0.0.0.0).
fn is_blocked_error(e: &std::io::Error) -> bool {
    cfg!(windows) && e.raw_os_error() == Some(11004)
}

const SMOKE_ATTEMPTS: u32 = 6;
const SMOKE_RETRY_DELAY: Duration = Duration::from_millis(500);

fn smoke_once(mode: FilterMode) -> bool {
    let (tx, rx) = mpsc::channel::<Vec<IpAddr>>();
    let target = format!("{}:53", mode.probe_host());
    std::thread::spawn(move || {
        let addrs = match target.to_socket_addrs() {
            Ok(it) => it.map(|a| a.ip()).collect::<Vec<_>>(),
            // Windows: o getaddrinfo descarta a resposta 0.0.0.0 do filtro e devolve
            // WSANO_DATA (11004). "Domínio existe mas sem dados" = bloqueado pelo filtro.
            Err(e) if is_blocked_error(&e) => vec![IpAddr::from([0, 0, 0, 0])],
            Err(_) => Vec::new(),
        };
        let _ = tx.send(addrs);
    });
    rx.recv_timeout(RESOLUTION_TIMEOUT)
        .map(|a| resolution_matches(&a, mode))
        .unwrap_or(false)
}

#[cfg(test)]
mod qa_tests {
    use super::*;
    use crate::config::BackendKind;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializa testes que mutam $HOME (variável de ambiente é global ao processo).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn unique_temp_home(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "guialar-qa-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ))
    }

    fn restore_home(old_home: Option<String>) {
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
    }

    /// QA-CONTRACT: sem manifesto => Unprotected (determinístico, sem rede).
    #[test]
    fn status_is_unprotected_without_manifest() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_home = std::env::var("HOME").ok();
        let home = unique_temp_home("unprotected");
        std::fs::create_dir_all(home.join(".config")).unwrap();
        std::env::set_var("HOME", &home);

        let status = check_status();

        restore_home(old_home);
        std::fs::remove_dir_all(&home).ok();
        assert_eq!(status, ProtectionStatus::Unprotected);
    }

    /// QA-CONTRACT: manifesto corrompido => PartiallyConfigured (nunca Protected).
    #[test]
    fn status_is_partial_on_corrupted_manifest() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_home = std::env::var("HOME").ok();
        let home = unique_temp_home("corrupt");
        let state_dir = home.join(".config").join("guialar");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::write(state_dir.join("state.json"), "{ corrompido").unwrap();
        std::env::set_var("HOME", &home);

        let status = check_status();

        restore_home(old_home);
        std::fs::remove_dir_all(&home).ok();
        match status {
            ProtectionStatus::PartiallyConfigured(msg) => assert!(
                msg.starts_with("Manifesto corrompido"),
                "mensagem inesperada: {}",
                msg
            ),
            other => panic!("esperado PartiallyConfigured, veio {:?}", other),
        }
    }

    /// QA-CONTRACT: manifesto válido salvo com save_atomic é recarregado (load).
    #[test]
    fn manifest_save_then_load_round_trip_on_disk() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_home = std::env::var("HOME").ok();
        let home = unique_temp_home("roundtrip");
        std::env::set_var("HOME", &home);

        let manifest = DnsStateManifest::new(BackendKind::SystemdResolved);
        manifest.save_atomic().expect("save_atomic deve funcionar");
        let loaded = DnsStateManifest::load().expect("load não deve falhar");

        restore_home(old_home);
        std::fs::remove_dir_all(&home).ok();
        assert!(loaded.is_some(), "manifesto salvo deve ser carregado");
        assert_eq!(loaded.unwrap().backend, BackendKind::SystemdResolved);
    }

    /// QA-001: DNS sequestrado ou que responde normalmente NÃO conta como filtro.
    #[test]
    fn qa001_so_resposta_bloqueada_conta_como_protegido() {
        let un = |s: &str| -> IpAddr { s.parse().unwrap() };
        for m in [FilterMode::Family, FilterMode::AdsOnly] {
            assert!(!resolution_matches(&[], m));
            assert!(!resolution_matches(&[un("127.0.0.1")], m));
            assert!(!resolution_matches(&[un("104.18.5.35")], m)); // resposta real: sem filtro
            assert!(!resolution_matches(&[un("1.1.1.3")], m)); // IP do filtro não prova o filtro
            assert!(resolution_matches(&[un("0.0.0.0")], m));
            assert!(resolution_matches(&[un("::")], m));
        }
    }

    #[test]
    fn permissao_negada_nao_e_corrompido() {
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let e = anyhow::Error::new(io).context("Falha ao ler manifesto");
        assert!(!describe_load_error(&e).starts_with("Manifesto corrompido"));
        let e = anyhow::anyhow!("json ruim");
        assert!(describe_load_error(&e).starts_with("Manifesto corrompido"));
    }

    #[test]
    fn dominios_de_teste_por_modo() {
        assert_eq!(FilterMode::Family.probe_host(), "nudity.testcategory.com");
        assert_eq!(FilterMode::AdsOnly.probe_host(), "doubleclick.net");
    }

    #[test]
    fn wsa_no_data_so_conta_no_windows() {
        let e = std::io::Error::from_raw_os_error(11004);
        assert_eq!(is_blocked_error(&e), cfg!(windows));
        assert!(!is_blocked_error(&std::io::Error::from_raw_os_error(11001)));
    }

    fn ips(v: &[&str]) -> Vec<IpAddr> {
        v.iter().map(|s| s.parse().unwrap()).collect()
    }

    /// Tabela do check_status (ADR-025), sem rede: (modo, DNS configurado, sonda) -> estado.
    #[test]
    fn tabela_do_check_status() {
        use FilterMode::{AdsOnly, Family};
        let fam4 = ["1.1.1.3", "1.0.0.3"];
        let fam6 = ["2606:4700:4700::1113", "2606:4700:4700::1003"];
        let ads4 = ["94.140.14.14", "94.140.15.15"];
        let todos_fam: Vec<&str> = fam4.iter().chain(fam6.iter()).copied().collect();
        let prot = |m, dns: &[&str], probe| decide_status(m, &ips(dns), || probe);
        let parcial = |s: ProtectionStatus| matches!(s, ProtectionStatus::PartiallyConfigured(_));

        assert_eq!(prot(Family, &todos_fam, true), ProtectionStatus::Protected);
        assert_eq!(prot(AdsOnly, &ads4, true), ProtectionStatus::Protected);
        // sonda falha com o DNS certo: parcial
        assert!(parcial(prot(Family, &todos_fam, false)));
        // 1.1.1.2 (só malware): a sonda nova já pega, mas o DNS de fora também é parcial
        assert!(parcial(prot(Family, &["1.1.1.2", "1.0.0.2"], true)));
        // DNS de outro modo
        assert!(parcial(prot(Family, &ads4, true)));
        assert!(parcial(prot(AdsOnly, &fam4, true)));
        // Pi-hole/roteador junto com o nosso: qualquer DNS de fora derruba
        assert!(parcial(prot(
            AdsOnly,
            &["94.140.14.14", "192.168.0.2"],
            true
        )));
        // IPv6 de fora com IPv4 nosso
        assert!(parcial(prot(
            Family,
            &["1.1.1.3", "2001:4860:4860::8888"],
            true
        )));
        // stubs locais não contam nem como de fora nem como nosso
        assert_eq!(
            prot(Family, &["127.0.0.53", "1.1.1.3"], true),
            ProtectionStatus::Protected
        );
        assert_eq!(
            prot(Family, &["127.0.0.53"], true),
            ProtectionStatus::Protected
        );
        assert!(parcial(prot(Family, &["127.0.0.53"], false)));
        assert_eq!(
            prot(Family, &["fec0:0:0:ffff::1", "1.1.1.3"], true),
            ProtectionStatus::Protected
        );
        // sem DNS legível: Parcial, mesmo com a sonda bloqueando
        assert_eq!(
            prot(Family, &[], true),
            ProtectionStatus::PartiallyConfigured(MSG_DNS_UNREADABLE.into())
        );
        assert!(parcial(prot(Family, &[], false)));
    }

    #[test]
    fn dns_de_fora_nao_gasta_rede() {
        let s = decide_status(FilterMode::Family, &ips(&["8.8.8.8"]), || {
            panic!("a sonda não deve rodar com DNS de fora")
        });
        match s {
            ProtectionStatus::PartiallyConfigured(m) => assert!(m.contains("8.8.8.8")),
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn sem_manifesto_continua_desprotegido() {
        let _g = ENV_LOCK.lock().unwrap();
        let old = std::env::var("HOME").ok();
        let home = unique_temp_home("semmanifesto");
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);
        std::env::remove_var("SUDO_USER");
        let st = check_status();
        restore_home(old);
        std::fs::remove_dir_all(&home).ok();
        assert_eq!(st, ProtectionStatus::Unprotected);
    }

    #[test]
    fn parcial_por_dns_de_fora_nao_cita_botao_proteger() {
        match decide_status(FilterMode::Family, &ips(&["8.8.8.8"]), || true) {
            ProtectionStatus::PartiallyConfigured(m) => {
                assert!(!m.contains("Proteger"), "{m}");
                assert!(m.contains("Desative a proteção e proteja de novo"));
            }
            o => panic!("{o:?}"),
        }
    }
}

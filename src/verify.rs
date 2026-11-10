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

pub fn check_status() -> ProtectionStatus {
    let manifest_opt = match DnsStateManifest::load() {
        Ok(m) => m,
        Err(e) => return ProtectionStatus::PartiallyConfigured(describe_load_error(&e)),
    };

    if manifest_opt.is_none() {
        return ProtectionStatus::Unprotected;
    }

    let mode = manifest_opt.map(|m| m.mode).unwrap_or_default();
    if run_dns_smoke_test(mode) {
        ProtectionStatus::Protected
    } else {
        ProtectionStatus::PartiallyConfigured(
            "DNS gravado, mas o filtro não respondeu no teste de rede".into(),
        )
    }
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

const SMOKE_ATTEMPTS: u32 = 6;
const SMOKE_RETRY_DELAY: Duration = Duration::from_millis(500);

fn smoke_once(mode: FilterMode) -> bool {
    let (tx, rx) = mpsc::channel::<Vec<IpAddr>>();
    let target = format!("{}:53", mode.probe_host());
    std::thread::spawn(move || {
        let addrs = target
            .to_socket_addrs()
            .map(|it| it.map(|a| a.ip()).collect::<Vec<_>>())
            .unwrap_or_default();
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
        assert_eq!(FilterMode::Family.probe_host(), "malware.testcategory.com");
        assert_eq!(FilterMode::AdsOnly.probe_host(), "doubleclick.net");
    }
}

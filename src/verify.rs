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
            "DNS gravado, mas a filtragem familiar não respondeu no teste de rede".into(),
        )
    }
}

/// `true` somente se a resolução contém um IP da Cloudflare Family.
///
/// Sem esta checagem, DNS sequestrado (127.0.0.1, proxy do provedor) era
/// reportado como "protegido" (QA-001).
#[cfg(test)]
pub fn resolution_is_family(addrs: &[IpAddr]) -> bool {
    resolution_matches(addrs, FilterMode::Family)
}

/// A resolução contém um IP do filtro do `mode` (pares nunca se misturam).
pub fn resolution_matches(addrs: &[IpAddr], mode: FilterMode) -> bool {
    addrs.iter().any(|ip| match ip {
        IpAddr::V4(v4) => mode.ipv4().contains(v4),
        IpAddr::V6(v6) => mode.ipv6().contains(v6),
    })
}

/// Smoke test com timeout: rede quebrada não pode travar a CLI.
fn run_dns_smoke_test(mode: FilterMode) -> bool {
    // Logo após `apply` o resolvedor leva algumas centenas de ms para estabilizar.
    for attempt in 0..SMOKE_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(SMOKE_RETRY_DELAY);
        }
        if smoke_once(mode) {
            return true;
        }
    }
    false
}

const SMOKE_ATTEMPTS: u32 = 3;
const SMOKE_RETRY_DELAY: Duration = Duration::from_millis(350);

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

    /// QA-001: só IPs Cloudflare Family contam como protegido.
    #[test]
    fn qa001_resolution_is_family_rejects_hijacked_dns() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        assert!(!resolution_is_family(&[]));
        assert!(!resolution_is_family(&["127.0.0.1".parse().unwrap()]));
        assert!(!resolution_is_family(&[IpAddr::V4(Ipv4Addr::new(
            1, 1, 1, 1
        ))]));
        assert!(!resolution_is_family(&[IpAddr::V6(Ipv6Addr::LOCALHOST)]));
        assert!(resolution_is_family(&["1.1.1.3".parse().unwrap()]));
        assert!(resolution_is_family(&[
            "127.0.0.1".parse().unwrap(),
            "1.0.0.3".parse().unwrap()
        ]));
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
    fn resolucao_por_modo_nao_mistura_pares() {
        let cf: Vec<IpAddr> = vec!["1.1.1.3".parse().unwrap()];
        let ag: Vec<IpAddr> = vec!["94.140.14.14".parse().unwrap()];
        assert!(resolution_matches(&cf, FilterMode::Family));
        assert!(!resolution_matches(&cf, FilterMode::AdsOnly));
        assert!(resolution_matches(&ag, FilterMode::AdsOnly));
        assert!(!resolution_matches(&ag, FilterMode::Family));
    }
}

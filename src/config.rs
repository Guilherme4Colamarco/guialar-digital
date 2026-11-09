//! Módulo de configuração global e persistência de estado.
//!
//! Guarda os IPs oficiais da Cloudflare for Families (IPv4 e IPv6)
//! e gerencia o manifesto atômico de reversão em formato JSON (~/.config/guialar/state.json).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

/// IPs Oficiais do Cloudflare 1.1.1.1 for Families (Bloqueio de Malware + Conteúdo Adulto).
pub const CLOUDFLARE_IPV4: [Ipv4Addr; 2] = [Ipv4Addr::new(1, 1, 1, 3), Ipv4Addr::new(1, 0, 0, 3)];

pub const CLOUDFLARE_IPV6: [Ipv6Addr; 2] = [
    Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1113),
    Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1003),
];

/// Backend utilizado no momento da aplicação do DNS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackendKind {
    NetworkManager,
    SystemdResolved,
    OpenResolvRunit,
    DirectResolvConf,
    WindowsPowerShell,
}

/// Backup do estado de uma conexão específica (usado no NetworkManager ou Windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionBackup {
    pub id: String,
    pub original_ipv4_dns: Vec<Ipv4Addr>,
    pub original_ipv6_dns: Vec<Ipv6Addr>,
    pub was_dhcp_ipv4: bool,
    pub was_dhcp_ipv6: bool,
}

/// Manifesto do estado anterior para garantir reversão segura (undo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStateManifest {
    pub backend: BackendKind,
    pub applied_at_utc: String,
    pub connections: Vec<ConnectionBackup>,
    pub previous_resolv_conf: Option<String>,
}

impl DnsStateManifest {
    pub fn new(backend: BackendKind) -> Self {
        Self {
            backend,
            applied_at_utc: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| format!("{}", d.as_secs()))
                .unwrap_or_else(|_| "0".into()),
            connections: Vec::new(),
            previous_resolv_conf: None,
        }
    }

    /// Retorna o caminho canônico do arquivo de estado conforme o sistema operacional.
    pub fn file_path() -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            let base = std::env::var("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(r"C:\ProgramData"));
            Ok(base.join("GuiaLar").join("state.json"))
        }

        #[cfg(not(target_os = "windows"))]
        {
            let home = crate::sysutil::effective_home()
                .context("Variável de ambiente $HOME não encontrada")?;
            Ok(home.join(".config").join("guialar").join("state.json"))
        }
    }

    /// Carrega o manifesto persistido. Retorna Ok(None) se não existir proteção ativa.
    pub fn load() -> Result<Option<Self>> {
        let path = Self::file_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Falha ao ler manifesto em {:?}", path))?;
        let manifest: Self = serde_json::from_str(&content)
            .with_context(|| {
                format!(
                    "Formato de manifesto corrompido ou inválido em {:?} (apague esse arquivo e rode 'guialar apply' de novo)",
                    path
                )
            })?;
        Ok(Some(manifest))
    }

    /// Salva o manifesto de forma atômica (grava em temp e faz rename) com permissão 0600.
    pub fn save_atomic(&self) -> Result<()> {
        let target_path = Self::file_path()?;
        let parent = target_path
            .parent()
            .context("Caminho de manifesto inválido sem diretório pai")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Falha ao criar diretório {:?}", parent))?;

        let temp_path = parent.join(format!(".state.{}.tmp", std::process::id()));
        let json_data = serde_json::to_string_pretty(self)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp_path)
                .with_context(|| format!("Falha ao criar arquivo temporário {:?}", temp_path))?;
            file.write_all(json_data.as_bytes())?;
            file.flush()?;
        }

        #[cfg(not(unix))]
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)
                .with_context(|| format!("Falha ao criar arquivo temporário {:?}", temp_path))?;
            file.write_all(json_data.as_bytes())?;
            file.flush()?;
        }

        fs::rename(&temp_path, &target_path)
            .with_context(|| format!("Falha ao renomear {:?} para {:?}", temp_path, target_path))?;
        Ok(())
    }

    /// Remove o manifesto após reversão bem-sucedida.
    pub fn remove_manifest() -> Result<()> {
        let path = Self::file_path()?;
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("Falha ao remover manifesto {:?}", path))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod qa_tests {
    use super::*;

    /// QA-CONTRACT: IPs Cloudflare Family são imutáveis (golden values).
    #[test]
    fn cloudflare_golden_ips() {
        assert_eq!(CLOUDFLARE_IPV4[0], Ipv4Addr::new(1, 1, 1, 3));
        assert_eq!(CLOUDFLARE_IPV4[1], Ipv4Addr::new(1, 0, 0, 3));
        assert_eq!(
            CLOUDFLARE_IPV6[0],
            Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1113)
        );
        assert_eq!(
            CLOUDFLARE_IPV6[1],
            Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1003)
        );
    }

    /// QA-CONTRACT: round-trip serde do manifesto (requisito de reversão segura).
    #[test]
    fn manifest_serde_round_trip() {
        let manifest = DnsStateManifest {
            backend: BackendKind::NetworkManager,
            applied_at_utc: "1730000000".into(),
            connections: vec![ConnectionBackup {
                id: "con-wifi-1".into(),
                original_ipv4_dns: vec![Ipv4Addr::new(192, 168, 0, 1), Ipv4Addr::new(8, 8, 8, 8)],
                original_ipv6_dns: vec![Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)],
                was_dhcp_ipv4: true,
                was_dhcp_ipv6: false,
            }],
            previous_resolv_conf: Some("nameserver 10.0.0.1\n".into()),
        };

        let json = serde_json::to_string(&manifest).expect("serializa");
        let parsed: DnsStateManifest = serde_json::from_str(&json).expect("desserializa");
        assert_eq!(parsed.backend, BackendKind::NetworkManager);
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.connections[0].original_ipv4_dns.len(), 2);
        assert_eq!(parsed.connections[0].original_ipv6_dns.len(), 1);
        assert!(parsed.connections[0].was_dhcp_ipv4);
        assert!(!parsed.connections[0].was_dhcp_ipv6);
        assert_eq!(
            parsed.previous_resolv_conf.as_deref(),
            Some("nameserver 10.0.0.1\n")
        );
    }

    /// QA-CONTRACT: nomes dos backends são estáveis no JSON (compatibilidade de manifesto).
    #[test]
    fn backend_kind_json_names_are_stable() {
        assert_eq!(
            serde_json::to_string(&BackendKind::NetworkManager).unwrap(),
            "\"NetworkManager\""
        );
        assert_eq!(
            serde_json::to_string(&BackendKind::SystemdResolved).unwrap(),
            "\"SystemdResolved\""
        );
        assert_eq!(
            serde_json::to_string(&BackendKind::OpenResolvRunit).unwrap(),
            "\"OpenResolvRunit\""
        );
        assert_eq!(
            serde_json::to_string(&BackendKind::DirectResolvConf).unwrap(),
            "\"DirectResolvConf\""
        );
        assert_eq!(
            serde_json::to_string(&BackendKind::WindowsPowerShell).unwrap(),
            "\"WindowsPowerShell\""
        );
    }

    /// QA-CONTRACT: manifesto novo nasce vazio com timestamp numérico (epoch UTC).
    #[test]
    fn new_manifest_has_empty_defaults() {
        let m = DnsStateManifest::new(BackendKind::DirectResolvConf);
        assert_eq!(m.backend, BackendKind::DirectResolvConf);
        assert!(m.connections.is_empty());
        assert!(m.previous_resolv_conf.is_none());
        assert!(
            m.applied_at_utc.parse::<u64>().is_ok(),
            "applied_at_utc deve ser epoch numérico, veio {:?}",
            m.applied_at_utc
        );
    }
}

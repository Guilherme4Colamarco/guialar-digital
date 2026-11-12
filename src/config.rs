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

/// IPs do AdGuard DNS "Default" (bloqueia anúncios e rastreadores, sem filtro de conteúdo adulto).
pub const ADGUARD_IPV4: [Ipv4Addr; 2] = [
    Ipv4Addr::new(94, 140, 14, 14),
    Ipv4Addr::new(94, 140, 15, 15),
];

pub const ADGUARD_IPV6: [Ipv6Addr; 2] = [
    Ipv6Addr::new(0x2a10, 0x50c0, 0, 0, 0, 0, 0xad1, 0xff),
    Ipv6Addr::new(0x2a10, 0x50c0, 0, 0, 0, 0, 0xad2, 0xff),
];

/// Modo de filtro DNS (ADR-022). Os pares de IPs nunca se misturam.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
pub enum FilterMode {
    /// Cloudflare for Families: malware + conteúdo adulto.
    #[default]
    #[serde(rename = "family")]
    #[value(name = "family")]
    Family,
    /// AdGuard DNS Default: só anúncios e rastreadores.
    #[serde(rename = "ads")]
    #[value(name = "ads", alias = "ads-only")]
    AdsOnly,
}

impl FilterMode {
    pub fn ipv4(self) -> [Ipv4Addr; 2] {
        match self {
            Self::Family => CLOUDFLARE_IPV4,
            Self::AdsOnly => ADGUARD_IPV4,
        }
    }

    pub fn ipv6(self) -> [Ipv6Addr; 2] {
        match self {
            Self::Family => CLOUDFLARE_IPV6,
            Self::AdsOnly => ADGUARD_IPV6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Family => "Proteção familiar",
            Self::AdsOnly => "Só bloquear anúncios",
        }
    }

    /// Domínio de teste que o filtro do modo bloqueia (responde 0.0.0.0).
    /// `nudity.testcategory.com` só é bloqueado pelo filtro família (1.1.1.3): o DNS que
    /// pega só malware (1.1.1.2) também bloquearia `malware.testcategory.com`.
    /// `doubleclick.net` é bloqueado pelo AdGuard DNS Padrão.
    pub fn probe_host(self) -> &'static str {
        match self {
            Self::Family => "nudity.testcategory.com",
            Self::AdsOnly => "doubleclick.net",
        }
    }

    /// Argumento da CLI (`--mode`).
    pub fn cli_name(self) -> &'static str {
        match self {
            Self::Family => "family",
            Self::AdsOnly => "ads",
        }
    }
}

/// `true` se o IPv4 pertence a QUALQUER filtro do GuiaLar (nunca vira "DNS original").
pub fn is_filter_ipv4(ip: &Ipv4Addr) -> bool {
    CLOUDFLARE_IPV4.contains(ip) || ADGUARD_IPV4.contains(ip)
}

#[cfg_attr(windows, allow(dead_code))]
pub fn is_filter_ipv6(ip: &Ipv6Addr) -> bool {
    CLOUDFLARE_IPV6.contains(ip) || ADGUARD_IPV6.contains(ip)
}

/// DNS local do próprio sistema (stub do systemd-resolved, dnsmasq do NetworkManager,
/// loopback do Windows): não é "DNS de fora" nem é o nosso; o que conta é o upstream.
pub fn is_local_stub(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_unspecified(),
        std::net::IpAddr::V6(v6) => {
            // ::1, :: e fec0::/10 (site-local, usado pelo Windows na interface de loopback)
            v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xffc0) == 0xfec0
        }
    }
}

/// A linha (resolv.conf etc.) cita algum IP de filtro do GuiaLar?
#[cfg_attr(windows, allow(dead_code))]
pub fn line_has_filter_ip(line: &str) -> bool {
    line.split_whitespace()
        .any(|t| match t.parse::<std::net::IpAddr>() {
            Ok(std::net::IpAddr::V4(v4)) => is_filter_ipv4(&v4),
            Ok(std::net::IpAddr::V6(v6)) => is_filter_ipv6(&v6),
            Err(_) => false,
        })
}

static MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Modo escolhido para esta execução (`apply --mode`, seletor da GUI).
pub fn set_mode(m: FilterMode) {
    MODE.store(
        matches!(m, FilterMode::AdsOnly) as u8,
        std::sync::atomic::Ordering::SeqCst,
    );
}

pub fn current_mode() -> FilterMode {
    if MODE.load(std::sync::atomic::Ordering::SeqCst) == 1 {
        FilterMode::AdsOnly
    } else {
        FilterMode::Family
    }
}

/// Backend utilizado no momento da aplicação do DNS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackendKind {
    NetworkManager,
    SystemdResolved,
    OpenResolvRunit,
    DirectResolvConf,
    WindowsPowerShell,
}

/// DNS "original" nunca pode ser o filtro do próprio GuiaLar (apply sem manifesto sobre
/// rede já filtrada).
///
/// - Os IPs de filtro saem da lista; os do usuário ficam (e voltam no revert).
/// - Lista que tinha IPs e só tinha filtro: a interface é tratada como DHCP, sem IPs fixos.
/// - Lista vazia desde o início: mantém `was_dhcp` como foi lido (no NetworkManager,
///   `ignore-auto-dns=yes` sem IP fixo é uma configuração do usuário e tem de voltar igual).
pub fn sanitize_original_dns(ips: Vec<Ipv4Addr>, was_dhcp: bool) -> (Vec<Ipv4Addr>, bool) {
    let tinha = !ips.is_empty();
    let fixos: Vec<Ipv4Addr> = ips.into_iter().filter(|ip| !is_filter_ipv4(ip)).collect();
    if tinha && fixos.is_empty() {
        (Vec::new(), true)
    } else {
        (fixos, was_dhcp)
    }
}

/// Igual a [`sanitize_original_dns`], para IPv6.
#[cfg_attr(windows, allow(dead_code))]
pub fn sanitize_original_dns6(ips: Vec<Ipv6Addr>, was_dhcp: bool) -> (Vec<Ipv6Addr>, bool) {
    let tinha = !ips.is_empty();
    let fixos: Vec<Ipv6Addr> = ips.into_iter().filter(|ip| !is_filter_ipv6(ip)).collect();
    if tinha && fixos.is_empty() {
        (Vec::new(), true)
    } else {
        (fixos, was_dhcp)
    }
}

fn default_true() -> bool {
    true
}

/// Backup do estado de uma conexão específica (usado no NetworkManager ou Windows).
///
/// `was_dhcp_*` é `true` quando a conexão aceitava o DNS da rede (NetworkManager:
/// `ignore-auto-dns=no`), mesmo que também tivesse IPs fixos em `original_*_dns`.
/// Manifestos antigos sem os campos de IPv6 leem como «DHCP, sem IPs fixos».
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionBackup {
    pub id: String,
    pub original_ipv4_dns: Vec<Ipv4Addr>,
    #[serde(default)]
    pub original_ipv6_dns: Vec<Ipv6Addr>,
    pub was_dhcp_ipv4: bool,
    #[serde(default = "default_true")]
    pub was_dhcp_ipv6: bool,
}

/// Manifesto do estado anterior para garantir reversão segura (undo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsStateManifest {
    pub backend: BackendKind,
    pub applied_at_utc: String,
    pub connections: Vec<ConnectionBackup>,
    pub previous_resolv_conf: Option<String>,
    /// Filtro ativo. Manifestos antigos (sem o campo) são `family`.
    #[serde(default)]
    pub mode: FilterMode,
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
            mode: current_mode(),
        }
    }

    /// Retorna o caminho canônico do arquivo de estado conforme o sistema operacional.
    pub fn file_path() -> Result<PathBuf> {
        let path = Self::file_path_raw()?;
        #[cfg(test)]
        {
            crate::testenv::assert_sandboxed("state.json");
            crate::testenv::assert_inside(&path, "state.json");
        }
        Ok(path)
    }

    fn file_path_raw() -> Result<PathBuf> {
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
        // Via sudo/pkexec o arquivo nasceria root:root 0600 e o usuário comum não o leria.
        #[cfg(unix)]
        {
            crate::sysutil::give_to_caller(parent);
            if let Some(config_dir) = parent.parent() {
                let root_owned = {
                    use std::os::unix::fs::MetadataExt;
                    fs::metadata(config_dir)
                        .map(|m| m.uid() == 0)
                        .unwrap_or(false)
                };
                if root_owned {
                    crate::sysutil::give_to_caller(config_dir);
                }
            }
            crate::sysutil::give_to_caller(&target_path);
        }
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
            mode: FilterMode::AdsOnly,
        };

        let json = serde_json::to_string(&manifest).expect("serializa");
        let parsed: DnsStateManifest = serde_json::from_str(&json).expect("desserializa");
        assert_eq!(parsed.backend, BackendKind::NetworkManager);
        assert_eq!(parsed.connections.len(), 1);
        assert_eq!(parsed.connections[0].original_ipv4_dns.len(), 2);
        assert_eq!(parsed.connections[0].original_ipv6_dns.len(), 1);
        assert!(parsed.connections[0].was_dhcp_ipv4);
        assert!(!parsed.connections[0].was_dhcp_ipv6);
        assert_eq!(parsed.mode, FilterMode::AdsOnly);
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

    #[test]
    fn dns_original_nunca_e_o_filtro() {
        let cf = vec![CLOUDFLARE_IPV4[0], CLOUDFLARE_IPV4[1]];
        assert_eq!(sanitize_original_dns(cf.clone(), false), (Vec::new(), true));
        assert_eq!(
            sanitize_original_dns(vec![CLOUDFLARE_IPV4[0]], false),
            (Vec::new(), true)
        );
        let casa = vec![Ipv4Addr::new(192, 168, 0, 1)];
        assert_eq!(
            sanitize_original_dns(casa.clone(), false),
            (casa.clone(), false)
        );
        // lista mista: o IP de filtro sai e só o do usuário volta no revert (N2)
        let misto = vec![CLOUDFLARE_IPV4[0], Ipv4Addr::new(9, 9, 9, 9)];
        assert_eq!(
            sanitize_original_dns(misto, false),
            (vec![Ipv4Addr::new(9, 9, 9, 9)], false)
        );
        assert_eq!(sanitize_original_dns(Vec::new(), true), (Vec::new(), true));
        // `ignore-auto-dns=yes` sem IP fixo é do usuário e não vira DHCP (N1)
        assert_eq!(
            sanitize_original_dns(Vec::new(), false),
            (Vec::new(), false)
        );
    }

    #[test]
    fn tabela_de_ips_por_modo() {
        assert_eq!(
            FilterMode::Family.ipv4(),
            [Ipv4Addr::new(1, 1, 1, 3), Ipv4Addr::new(1, 0, 0, 3)]
        );
        assert_eq!(
            FilterMode::AdsOnly.ipv4(),
            [
                Ipv4Addr::new(94, 140, 14, 14),
                Ipv4Addr::new(94, 140, 15, 15)
            ]
        );
        assert_eq!(
            FilterMode::AdsOnly.ipv6()[0].to_string(),
            "2a10:50c0::ad1:ff"
        );
        assert_eq!(
            FilterMode::AdsOnly.ipv6()[1].to_string(),
            "2a10:50c0::ad2:ff"
        );
        assert_eq!(
            FilterMode::Family.ipv6()[0].to_string(),
            "2606:4700:4700::1113"
        );
        for ip in FilterMode::Family.ipv4() {
            assert!(!FilterMode::AdsOnly.ipv4().contains(&ip));
        }
    }

    #[test]
    fn manifesto_antigo_sem_mode_e_family() {
        let antigo = r#"{"backend":"NetworkManager","applied_at_utc":"1","connections":[],"previous_resolv_conf":null}"#;
        let m: DnsStateManifest = serde_json::from_str(antigo).unwrap();
        assert_eq!(m.mode, FilterMode::Family);
        let novo = r#"{"backend":"NetworkManager","applied_at_utc":"1","connections":[],"previous_resolv_conf":null,"mode":"ads"}"#;
        let m: DnsStateManifest = serde_json::from_str(novo).unwrap();
        assert_eq!(m.mode, FilterMode::AdsOnly);
        assert!(serde_json::to_string(&m)
            .unwrap()
            .contains("\"mode\":\"ads\""));
    }

    #[test]
    fn sanitize_reconhece_os_dois_conjuntos() {
        let ads = vec![ADGUARD_IPV4[0], ADGUARD_IPV4[1]];
        assert_eq!(sanitize_original_dns(ads, false), (Vec::new(), true));
        let misto = vec![CLOUDFLARE_IPV4[0], ADGUARD_IPV4[0]];
        assert_eq!(sanitize_original_dns(misto, false), (Vec::new(), true));
        let casa = vec![Ipv4Addr::new(192, 168, 0, 1), ADGUARD_IPV4[0]];
        assert_eq!(
            sanitize_original_dns(casa, false),
            (vec![Ipv4Addr::new(192, 168, 0, 1)], false)
        );
    }

    #[test]
    fn linha_com_ip_de_filtro() {
        assert!(line_has_filter_ip("nameserver 94.140.14.14"));
        assert!(line_has_filter_ip("nameserver 1.1.1.3"));
        assert!(line_has_filter_ip("nameserver 2a10:50c0::ad2:ff"));
        assert!(!line_has_filter_ip("nameserver 10.0.2.3"));
        assert!(!line_has_filter_ip("# 94.140.14.14x"));
    }

    #[test]
    fn manifesto_antigo_sem_campos_opcionais_migra() {
        // v0.2: sem `mode` e sem `previous_resolv_conf`
        let antigo = r#"{"backend":"DirectResolvConf","applied_at_utc":"1","connections":[]}"#;
        let m: DnsStateManifest = serde_json::from_str(antigo).unwrap();
        assert_eq!(m.mode, FilterMode::Family);
        assert!(m.previous_resolv_conf.is_none());
        // ao regravar, o manifesto já sai no formato novo e relê igual
        let novo = serde_json::to_string(&m).unwrap();
        assert!(novo.contains("\"mode\":\"family\""));
        let de_novo: DnsStateManifest = serde_json::from_str(&novo).unwrap();
        assert_eq!(de_novo.backend, BackendKind::DirectResolvConf);
        assert_eq!(de_novo.mode, FilterMode::Family);
    }

    #[test]
    fn manifesto_com_conexoes_antigas_preserva_o_dns_original() {
        let antigo = r#"{"backend":"NetworkManager","applied_at_utc":"1","connections":[{"id":"u-1","original_ipv4_dns":["192.168.0.1"],"original_ipv6_dns":[],"was_dhcp_ipv4":false,"was_dhcp_ipv6":true}],"previous_resolv_conf":null}"#;
        let m: DnsStateManifest = serde_json::from_str(antigo).unwrap();
        assert_eq!(m.connections.len(), 1);
        assert_eq!(
            m.connections[0].original_ipv4_dns,
            [Ipv4Addr::new(192, 168, 0, 1)]
        );
        assert!(!m.connections[0].was_dhcp_ipv4);
    }

    #[test]
    fn manifesto_ignora_campo_desconhecido_e_rejeita_valor_invalido() {
        let futuro = r#"{"backend":"SystemdResolved","applied_at_utc":"1","connections":[],"previous_resolv_conf":null,"mode":"ads","campo_novo":42}"#;
        let m: DnsStateManifest = serde_json::from_str(futuro).unwrap();
        assert_eq!(m.mode, FilterMode::AdsOnly);
        let backend_ruim = r#"{"backend":"Inexistente","applied_at_utc":"1","connections":[]}"#;
        assert!(serde_json::from_str::<DnsStateManifest>(backend_ruim).is_err());
        let modo_ruim =
            r#"{"backend":"NetworkManager","applied_at_utc":"1","connections":[],"mode":"xyz"}"#;
        assert!(serde_json::from_str::<DnsStateManifest>(modo_ruim).is_err());
        assert!(serde_json::from_str::<DnsStateManifest>("{}").is_err());
    }
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn backup_sem_campos_v6_le_como_dhcp_sem_fixos() {
        let b: ConnectionBackup = serde_json::from_str(
            r#"{"id":"u","original_ipv4_dns":["8.8.8.8"],"was_dhcp_ipv4":false}"#,
        )
        .unwrap();
        assert!(b.original_ipv6_dns.is_empty());
        assert!(b.was_dhcp_ipv6);
    }

    #[test]
    fn sanitize_v6_filtro_vira_automatico_e_fixo_externo_fica() {
        let filtro: Vec<Ipv6Addr> = CLOUDFLARE_IPV6.to_vec();
        assert_eq!(sanitize_original_dns6(filtro, false), (Vec::new(), true));
        // `ignore-auto-dns=yes` sem IP fixo é do usuário e não vira DHCP (N1)
        assert_eq!(
            sanitize_original_dns6(Vec::new(), false),
            (Vec::new(), false)
        );
        assert_eq!(sanitize_original_dns6(Vec::new(), true), (Vec::new(), true));
        let fixo: Ipv6Addr = "2001:db8::99".parse().unwrap();
        assert_eq!(sanitize_original_dns6(vec![fixo], true), (vec![fixo], true));
        assert_eq!(
            sanitize_original_dns6(vec![fixo], false),
            (vec![fixo], false)
        );
        // lista mista: o filtro sai, o do usuário fica (N2)
        assert_eq!(
            sanitize_original_dns6(vec![CLOUDFLARE_IPV6[0], fixo], false),
            (vec![fixo], false)
        );
    }
}

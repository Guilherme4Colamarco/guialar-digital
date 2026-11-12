//! Módulo de configuração global e persistência de estado.
//!
//! Guarda os IPs oficiais da Cloudflare for Families (IPv4 e IPv6)
//! e gerencia o manifesto atômico de reversão em formato JSON (~/.config/guialar/state.json).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
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

    /// Caminho do arquivo de estado, numa pasta do sistema (ADR-031): `/var/lib/guialar` no
    /// Linux, `%ProgramData%\GuiaLar` no Windows. Só o processo elevado grava; o resto só lê.
    pub fn file_path() -> Result<PathBuf> {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json");
        let path = crate::statedir::state_file()?;
        #[cfg(test)]
        crate::testenv::assert_inside(&path, "state.json");
        Ok(path)
    }

    /// Carrega o manifesto persistido. Retorna Ok(None) se não existir proteção ativa (ou se o
    /// arquivo não é de dono confiável: ver `statedir`).
    pub fn load() -> Result<Option<Self>> {
        let path = Self::file_path()?;
        let Some(bytes) = crate::statedir::read_state()? else {
            return Ok(None);
        };
        let manifest: Self = serde_json::from_slice(&bytes)
            .with_context(|| {
                format!(
                    "Formato de manifesto corrompido ou inválido em {:?} (apague esse arquivo e rode 'guialar apply' de novo)",
                    path
                )
            })?;
        Ok(Some(manifest))
    }

    /// Confere, antes de mexer no DNS, que o estado pode ser guardado com segurança. Se não
    /// puder, o `apply` para ali e nada é alterado. Só o processo elevado a chama.
    pub fn preflight() -> Result<()> {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json");
        crate::statedir::ensure_dir().map(|_| ())
    }

    /// Salva o manifesto de forma atômica na pasta do sistema (só o processo elevado).
    pub fn save_atomic(&self) -> Result<()> {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json");
        let json = serde_json::to_string_pretty(self)?;
        crate::statedir::write_state(json.as_bytes())
    }

    /// Remove o manifesto após reversão bem-sucedida.
    pub fn remove_manifest() -> Result<()> {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json");
        crate::statedir::remove_state()
    }

    // ------------------------------------------------------------ migração da beta (ADR-031)

    /// Onde a beta guardava o manifesto: na área do usuário. Só o `revert` o lê, uma vez.
    fn legacy_location() -> Option<(PathBuf, Vec<&'static str>)> {
        #[cfg(windows)]
        {
            let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
            Some((base, vec!["GuiaLar"]))
        }
        #[cfg(not(windows))]
        {
            let home = crate::sysutil::effective_home()?;
            Some((home, vec![".config", "guialar"]))
        }
    }

    /// Lê o manifesto antigo sem seguir links; recusa link e dono diferente (`Err`).
    pub fn load_legacy() -> Result<Option<Self>> {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json (antigo)");
        let Some((base, comps)) = Self::legacy_location() else {
            return Ok(None);
        };
        let Some(bytes) =
            crate::safefs::read_file(&base, &comps, "state.json", &crate::safefs::caller_owner())?
        else {
            return Ok(None);
        };
        if bytes.len() > 256 * 1024 {
            bail!("o registro antigo é grande demais para ser um manifesto");
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .context("o registro antigo está ilegível")
    }

    /// Apaga o manifesto antigo (só o nome; um link é removido sem tocar no alvo).
    pub fn remove_legacy() {
        #[cfg(test)]
        crate::testenv::assert_sandboxed("state.json (antigo)");
        if let Some((base, comps)) = Self::legacy_location() {
            let _ = crate::safefs::remove_file(
                &base,
                &comps,
                "state.json",
                &crate::safefs::caller_owner(),
            );
        }
    }

    /// O manifesto antigo é do usuário e pode ter sido forjado: antes de o processo elevado
    /// agir sobre ele, confere que cada conexão existe agora (`known_ids`), que os DNS são
    /// IPs razoáveis e que `previous_resolv_conf` só tem linhas permitidas e cabe em 4 KB.
    pub fn validate_legacy(&self, known_ids: &[String]) -> std::result::Result<(), String> {
        let compat = if cfg!(windows) {
            self.backend == BackendKind::WindowsPowerShell
        } else {
            self.backend != BackendKind::WindowsPowerShell
        };
        if !compat {
            return Err("o backend do registro não é o deste sistema".into());
        }
        if self.connections.len() > 32 {
            return Err("conexões demais".into());
        }
        for c in &self.connections {
            if !known_ids.contains(&c.id) {
                return Err(format!("a conexão {:?} não existe mais", short(&c.id)));
            }
            #[cfg(windows)]
            if !crate::sysutil::is_safe_ps_alias(&c.id) {
                return Err(format!("alias inválido: {:?}", short(&c.id)));
            }
            if c.original_ipv4_dns.len() > 8 || c.original_ipv6_dns.len() > 8 {
                return Err("DNS demais numa conexão".into());
            }
        }
        if let Some(t) = &self.previous_resolv_conf {
            validate_resolv_conf(t)?;
        }
        Ok(())
    }
}

fn short(s: &str) -> String {
    s.chars().take(40).collect()
}

/// `resolv.conf` antigo: só `nameserver <ip>`, `search`, `domain` e `options` (e comentários),
/// até 4 KB, sem bytes de controle.
pub fn validate_resolv_conf(text: &str) -> std::result::Result<(), String> {
    if text.len() > 4096 {
        return Err("o resolv.conf guardado passa de 4 KB".into());
    }
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if l.chars().any(|c| c.is_control() && c != '\t') {
            return Err("o resolv.conf guardado tem caracteres de controle".into());
        }
        let mut it = l.split_whitespace();
        match it.next() {
            Some("nameserver") => {
                let ok = it
                    .next()
                    .and_then(|ip| ip.split('%').next())
                    .is_some_and(|ip| ip.parse::<std::net::IpAddr>().is_ok())
                    && it.next().is_none();
                if !ok {
                    return Err(format!("linha nameserver inválida: {:?}", short(l)));
                }
            }
            Some("search") | Some("domain") | Some("options") => {}
            _ => {
                return Err(format!(
                    "linha não permitida no resolv.conf: {:?}",
                    short(l)
                ))
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod legacy_tests {
    use super::*;

    fn ids(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    fn base() -> DnsStateManifest {
        let mut m = DnsStateManifest::new(BackendKind::NetworkManager);
        m.connections.push(ConnectionBackup {
            id: "uuid-1".into(),
            original_ipv4_dns: vec![Ipv4Addr::new(192, 168, 0, 1)],
            original_ipv6_dns: vec![],
            was_dhcp_ipv4: false,
            was_dhcp_ipv6: true,
        });
        m
    }

    #[cfg(not(windows))]
    #[test]
    fn manifesto_antigo_valido_passa() {
        let mut m = base();
        m.previous_resolv_conf =
            Some("# Generated by NetworkManager\nsearch lan\nnameserver 192.168.0.1\nnameserver fe80::1%eth0\noptions edns0\n".into());
        assert_eq!(m.validate_legacy(&ids(&["uuid-1"])), Ok(()));
    }

    #[cfg(not(windows))]
    #[test]
    fn manifesto_antigo_com_conexao_que_nao_existe_e_recusado() {
        let m = base();
        let e = m.validate_legacy(&ids(&["outra"])).unwrap_err();
        assert!(e.contains("não existe mais"), "{e}");
        assert!(m.validate_legacy(&[]).is_err());
    }

    #[test]
    fn resolv_conf_so_aceita_linhas_permitidas_ate_4kb() {
        assert!(validate_resolv_conf(
            "nameserver 1.1.1.1\n\n# c\n; c\nsearch a.b c.d\ndomain x\noptions rotate\n"
        )
        .is_ok());
        for ruim in [
            "nameserver 1.1.1.1 extra",
            "nameserver abc",
            "nameserver",
            "curl x | sh",
            "nameserver 1.1.1.1\n$(rm -rf /)",
            "nameserver 1.1.1.1\u{0}",
        ] {
            assert!(validate_resolv_conf(ruim).is_err(), "{ruim:?}");
        }
        assert!(validate_resolv_conf(&"#".repeat(4097)).is_err());
        assert!(validate_resolv_conf(&"#".repeat(4096)).is_ok());
    }

    #[test]
    fn manifesto_antigo_de_outro_sistema_ou_com_dns_demais_e_recusado() {
        let mut m = base();
        m.backend = if cfg!(windows) {
            BackendKind::NetworkManager
        } else {
            BackendKind::WindowsPowerShell
        };
        assert!(m.validate_legacy(&ids(&["uuid-1"])).is_err());
        #[cfg(not(windows))]
        {
            let mut m = base();
            m.connections[0].original_ipv4_dns = vec![Ipv4Addr::new(9, 9, 9, 9); 9];
            assert!(m.validate_legacy(&ids(&["uuid-1"])).is_err());
        }
    }
}

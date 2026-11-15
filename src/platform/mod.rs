//! Despacho de plataforma em tempo de compilação.

pub mod runner;
pub(crate) mod transaction;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::config::DnsStateManifest;
use anyhow::Result;

/// O que fazer num `apply` quando já existe manifesto (ADR-027).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyPlan {
    /// Sem manifesto: aplica do zero.
    Fresh,
    /// Mesmo modo e o DNS configurado é o do modo: nada a fazer.
    AlreadyActive,
    /// Outro modo, ou o DNS foi trocado por fora / não foi lido: reaplica o DNS do modo,
    /// mantendo o DNS original gravado no manifesto.
    Reapply,
}

pub fn plan_apply(
    manifest: Option<&DnsStateManifest>,
    wanted: crate::config::FilterMode,
    configured: &[std::net::IpAddr],
) -> ApplyPlan {
    use crate::verify::{check_configured_dns, DnsCheck};
    match manifest {
        None => ApplyPlan::Fresh,
        Some(m)
            if m.mode == wanted && check_configured_dns(wanted, configured) == DnsCheck::Ours =>
        {
            ApplyPlan::AlreadyActive
        }
        Some(_) => ApplyPlan::Reapply,
    }
}

/// Primeiro passo de todo «proteger»: olha o manifesto e decide. É a ÚNICA decisão; quem a
/// executa é `crate::protection::protect`, usado pela linha de comando, pela janela e pelo
/// processo elevado.
pub enum ApplyStep {
    /// Já protegido com o DNS do modo: nada a fazer.
    AlreadyActive,
    /// Há manifesto: reaplica o DNS do modo, mantendo o DNS original gravado.
    Switch(DnsStateManifest),
    /// Sem manifesto: aplica do zero.
    Fresh,
    /// Sobrou um registro de uma proteção que já não está ligada (por exemplo, o desfazer
    /// terminou mas não conseguiu apagar o registro): nenhum DNS em uso é do filtro. Aplica do
    /// zero, que reinspeciona a rede e grava um registro novo, em vez de trocar de modo sobre
    /// um estado velho (adaptador novo ficaria sem filtro, DNS trocado depois se perderia) e
    /// religar a proteção sem o usuário pedir.
    Stale,
}

pub fn next_apply_step(configured: impl FnOnce() -> Vec<std::net::IpAddr>) -> Result<ApplyStep> {
    let Some(m) = DnsStateManifest::load()? else {
        return Ok(ApplyStep::Fresh);
    };
    let configured = configured();
    // Sem nenhum DNS legível (ou só o stub local) não dá para saber: mantém o registro.
    let reais: Vec<_> = configured
        .iter()
        .filter(|ip| !crate::config::is_local_stub(ip))
        .collect();
    if !reais.is_empty() && !reais.iter().any(|ip| crate::config::is_filter_ip(ip)) {
        return Ok(ApplyStep::Stale);
    }
    if plan_apply(Some(&m), crate::config::current_mode(), &configured) == ApplyPlan::AlreadyActive
    {
        return Ok(ApplyStep::AlreadyActive);
    }
    Ok(ApplyStep::Switch(m))
}

/// Windows: reabre como administrador (UAC) para `subcomando`. `true` = já executado lá.
#[cfg(target_os = "windows")]
pub fn elevate_if_needed(subcomando: &str) -> Result<bool> {
    windows::elevate_if_needed(subcomando)
}

/// Algum DNS em uso é o filtro do GuiaLar? (para recuperar sem manifesto). Usa a mesma leitura
/// do estado e do `plan_apply` (`configured_dns`), para as respostas não divergirem.
pub fn filter_dns_in_use() -> bool {
    configured_dns().iter().any(crate::config::is_filter_ip)
}

/// O sistema de verdade por trás das operações de proteção (`crate::protection`). É o único
/// lugar que escolhe o backend por SO; cada método eleva sozinho quando precisa.
pub struct System;

#[cfg(target_os = "linux")]
impl crate::protection::Engine for System {
    fn configured_dns(&self) -> Vec<std::net::IpAddr> {
        configured_dns()
    }
    /// No Linux o bloqueador dos navegadores é um passo à parte (abre a loja), fora da elevação.
    fn apply(&self, _with_browsers: bool) -> Result<()> {
        linux::apply_dns()
    }
    fn switch(&self, record: DnsStateManifest) -> Result<()> {
        linux::switch_mode(record)
    }
    fn revert(&self, record: &DnsStateManifest) -> Result<()> {
        linux::revert_dns(record)
    }
    fn revert_without_record(&self) -> Result<()> {
        linux::revert_dns_fallback()
    }
    fn revert_browsers_only(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl crate::protection::Engine for System {
    fn configured_dns(&self) -> Vec<std::net::IpAddr> {
        configured_dns()
    }
    /// `with_browsers`: DNS e políticas dos navegadores numa única elevação (um só UAC).
    fn apply(&self, with_browsers: bool) -> Result<()> {
        if with_browsers {
            windows::apply_all()
        } else {
            windows::apply_dns()
        }
    }
    fn switch(&self, record: DnsStateManifest) -> Result<()> {
        windows::switch_mode(record)
    }
    fn revert(&self, record: &DnsStateManifest) -> Result<()> {
        windows::revert_dns(record)
    }
    fn revert_without_record(&self) -> Result<()> {
        windows::revert_dns_fallback()
    }
    /// Sem DNS para reverter, ainda pode haver políticas do bloqueador.
    fn revert_browsers_only(&self) -> Result<()> {
        if windows::elevate_if_needed("internal-revert")? {
            return Ok(());
        }
        crate::policy::revert_all()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl crate::protection::Engine for System {
    fn configured_dns(&self) -> Vec<std::net::IpAddr> {
        Vec::new()
    }
    fn apply(&self, _with_browsers: bool) -> Result<()> {
        unsupported()
    }
    fn switch(&self, _record: DnsStateManifest) -> Result<()> {
        unsupported()
    }
    fn revert(&self, _record: &DnsStateManifest) -> Result<()> {
        unsupported()
    }
    fn revert_without_record(&self) -> Result<()> {
        unsupported()
    }
    fn revert_browsers_only(&self) -> Result<()> {
        unsupported()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn unsupported() -> Result<()> {
    anyhow::bail!("Sistema operacional não suportado nativamente pelo GuiaLar")
}

/// DNS efetivamente configurados na máquina (IPv4 e IPv6, inclusive stubs locais; quem
/// decide o que conta é `verify::check_configured_dns`). Falha de leitura = lista vazia.
pub fn configured_dns() -> Vec<std::net::IpAddr> {
    // Só em build de depuração (testes de contrato): DNS fixo, sem tocar na rede do host.
    #[cfg(debug_assertions)]
    if let Ok(v) = std::env::var("GUIALAR_TEST_DNS") {
        return v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
    }
    #[cfg(target_os = "linux")]
    {
        linux::configured_dns()
    }
    #[cfg(target_os = "windows")]
    {
        windows::configured_dns()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    use crate::config::{BackendKind, FilterMode};

    fn m(mode: FilterMode) -> DnsStateManifest {
        let mut x = DnsStateManifest::new(BackendKind::NetworkManager);
        x.mode = mode;
        x
    }

    fn ips(v: &[&str]) -> Vec<std::net::IpAddr> {
        v.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn apply_com_manifesto_so_diz_ja_ativa_se_o_dns_e_do_modo() {
        let fam = ips(&["1.1.1.3", "1.0.0.3"]);
        assert_eq!(plan_apply(None, FilterMode::Family, &fam), ApplyPlan::Fresh);
        assert_eq!(
            plan_apply(Some(&m(FilterMode::Family)), FilterMode::Family, &fam),
            ApplyPlan::AlreadyActive
        );
        // DNS trocado por fora: reaplica (nunca "já está ativa")
        assert_eq!(
            plan_apply(
                Some(&m(FilterMode::Family)),
                FilterMode::Family,
                &ips(&["8.8.8.8"])
            ),
            ApplyPlan::Reapply
        );
        // DNS ilegível: reaplica
        assert_eq!(
            plan_apply(Some(&m(FilterMode::Family)), FilterMode::Family, &[]),
            ApplyPlan::Reapply
        );
        // outro modo: troca
        assert_eq!(
            plan_apply(Some(&m(FilterMode::Family)), FilterMode::AdsOnly, &fam),
            ApplyPlan::Reapply
        );
    }

    #[test]
    fn tabela_completa_do_plan_apply() {
        use crate::config::{ADGUARD_IPV4, ADGUARD_IPV6, CLOUDFLARE_IPV4, CLOUDFLARE_IPV6};
        let fam: Vec<std::net::IpAddr> = CLOUDFLARE_IPV4.iter().map(|i| (*i).into()).collect();
        let ads: Vec<std::net::IpAddr> = ADGUARD_IPV4.iter().map(|i| (*i).into()).collect();
        let fam6: Vec<std::net::IpAddr> = CLOUDFLARE_IPV6.iter().map(|i| (*i).into()).collect();
        let ads6: Vec<std::net::IpAddr> = ADGUARD_IPV6.iter().map(|i| (*i).into()).collect();
        let (f, a) = (FilterMode::Family, FilterMode::AdsOnly);

        // sem manifesto: sempre do zero, seja qual for o DNS
        for dns in [&fam, &ads, &vec![]] {
            assert_eq!(plan_apply(None, f, dns), ApplyPlan::Fresh);
            assert_eq!(plan_apply(None, a, dns), ApplyPlan::Fresh);
        }
        // DNS do modo (v4, v6 ou os dois): nada a fazer
        assert_eq!(plan_apply(Some(&m(a)), a, &ads), ApplyPlan::AlreadyActive);
        assert_eq!(plan_apply(Some(&m(f)), f, &fam6), ApplyPlan::AlreadyActive);
        assert_eq!(plan_apply(Some(&m(a)), a, &ads6), ApplyPlan::AlreadyActive);
        let ambos: Vec<_> = fam.iter().chain(fam6.iter()).copied().collect();
        assert_eq!(plan_apply(Some(&m(f)), f, &ambos), ApplyPlan::AlreadyActive);
        // stub local (127.0.0.53) não conta contra nem a favor
        let com_stub = ips(&["127.0.0.53", "1.1.1.3"]);
        assert_eq!(
            plan_apply(Some(&m(f)), f, &com_stub),
            ApplyPlan::AlreadyActive
        );
        // DNS de fora, sozinho ou misturado com o do modo: reaplica
        assert_eq!(plan_apply(Some(&m(f)), f, &ads), ApplyPlan::Reapply);
        assert_eq!(plan_apply(Some(&m(a)), a, &fam), ApplyPlan::Reapply);
        let misto = ips(&["1.1.1.3", "8.8.8.8"]);
        assert_eq!(plan_apply(Some(&m(f)), f, &misto), ApplyPlan::Reapply);
        // DNS ilegível (vazio ou só stub): reaplica
        assert_eq!(plan_apply(Some(&m(a)), a, &[]), ApplyPlan::Reapply);
        let so_stub = ips(&["127.0.0.53", "::1"]);
        assert_eq!(plan_apply(Some(&m(f)), f, &so_stub), ApplyPlan::Reapply);
        // troca de modo, mesmo com o DNS do modo antigo em uso: reaplica
        assert_eq!(plan_apply(Some(&m(f)), a, &fam), ApplyPlan::Reapply);
        assert_eq!(plan_apply(Some(&m(a)), f, &ads), ApplyPlan::Reapply);
    }
}

#[cfg(test)]
mod apply_step_tests {
    use super::*;
    use crate::config::{BackendKind, FilterMode};
    use crate::testenv::Sandbox;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn passo(configurado: &[&str]) -> ApplyStep {
        let ips: Vec<IpAddr> = configurado.iter().map(|s| ip(s)).collect();
        next_apply_step(|| ips).unwrap()
    }

    fn com_registro(nome: &str) -> Sandbox {
        let s = Sandbox::new(nome);
        DnsStateManifest::new(BackendKind::DirectResolvConf)
            .save_atomic()
            .unwrap();
        s
    }

    #[test]
    fn sem_registro_aplica_do_zero() {
        let _s = Sandbox::new("passo-sem-registro");
        assert!(matches!(passo(&["192.168.0.1"]), ApplyStep::Fresh));
    }

    /// N3: sobrou um registro (o desfazer não conseguiu apagá-lo) mas o DNS em uso já é o da
    /// rede: aplica do zero, para reinspecionar, e não religa um modo antigo.
    #[test]
    fn registro_velho_com_dns_da_rede_aplica_do_zero() {
        let _s = com_registro("passo-registro-velho");
        assert!(matches!(passo(&["192.168.0.1"]), ApplyStep::Stale));
        assert!(matches!(
            passo(&["192.168.0.1", "2001:db8::1"]),
            ApplyStep::Stale
        ));
        // o stub local do sistema não conta como DNS «de fora» nem como o nosso
        assert!(matches!(
            passo(&["127.0.0.53", "8.8.8.8"]),
            ApplyStep::Stale
        ));
    }

    #[test]
    fn registro_com_dns_do_filtro_continua_sendo_troca_ou_ja_ativo() {
        let _s = com_registro("passo-registro-ativo");
        let modo = crate::config::current_mode();
        let v4 = modo.ipv4()[0].to_string();
        // o modo pedido e o do registro, e o DNS é o do filtro: já ativo
        let m = DnsStateManifest::load().unwrap().unwrap();
        assert_eq!(m.mode, modo);
        assert!(matches!(passo(&[&v4]), ApplyStep::AlreadyActive));
        // DNS de outro modo do filtro ainda é «proteção ligada»: troca de modo
        let outro = match modo {
            FilterMode::Family => FilterMode::AdsOnly,
            FilterMode::AdsOnly => FilterMode::Family,
        };
        let v4_outro = outro.ipv4()[0].to_string();
        assert!(matches!(passo(&[&v4_outro]), ApplyStep::Switch(_)));
        // filtro misturado com um DNS de fora também não é «desligada»
        assert!(matches!(
            passo(&[&v4_outro, "8.8.8.8"]),
            ApplyStep::Switch(_)
        ));
    }

    #[test]
    fn registro_sem_dns_legivel_nao_e_tratado_como_desligado() {
        let _s = com_registro("passo-registro-sem-dns");
        // nada lido, ou só o stub local: não dá para saber, mantém o registro (troca de modo)
        assert!(matches!(passo(&[]), ApplyStep::Switch(_)));
        assert!(matches!(passo(&["127.0.0.53"]), ApplyStep::Switch(_)));
    }
}

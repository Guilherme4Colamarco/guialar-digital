//! Despacho de plataforma em tempo de compilação.

pub mod runner;

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

/// Primeiro passo de todo `apply`: olha o manifesto e decide.
enum ApplyStep {
    /// Já protegido com o DNS do modo: nada a fazer.
    AlreadyActive,
    /// Há manifesto: reaplica o DNS do modo, mantendo o DNS original gravado.
    Switch(DnsStateManifest),
    /// Sem manifesto: aplica do zero.
    Fresh,
}

fn next_apply_step(configured: impl FnOnce() -> Vec<std::net::IpAddr>) -> Result<ApplyStep> {
    let Some(m) = DnsStateManifest::load()? else {
        return Ok(ApplyStep::Fresh);
    };
    if plan_apply(Some(&m), crate::config::current_mode(), &configured())
        == ApplyPlan::AlreadyActive
    {
        println!("ℹ️  A proteção já está ativa. Use 'guialar revert' antes de aplicar de novo.");
        return Ok(ApplyStep::AlreadyActive);
    }
    Ok(ApplyStep::Switch(m))
}

pub fn apply_dns() -> Result<()> {
    let _guard = crate::sysutil::guard_operation()?;
    // Idempotente: um segundo `apply` regravaria o manifesto com o DNS já da Cloudflare
    // como "original" e o `revert` nunca restauraria o DNS verdadeiro.
    match next_apply_step(configured_dns)? {
        ApplyStep::AlreadyActive => return Ok(()),
        ApplyStep::Switch(m) => return switch_mode(m), // mantém o DNS original do manifesto
        ApplyStep::Fresh => {}
    }
    #[cfg(target_os = "linux")]
    {
        linux::apply_dns()
    }

    #[cfg(target_os = "windows")]
    {
        windows::apply_dns()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("Sistema operacional não suportado nativamente pelo GuiaLar");
    }
}

/// Migra o manifesto antigo da beta, que ficava na área do usuário (ADR-031): só o `revert`
/// elevado o lê, uma vez, e só depois de validar (a conexão existe agora, DNS são IPs,
/// `previous_resolv_conf` só tem linhas permitidas). Válido: desfaz e apaga o antigo. Inválido:
/// não usa nada dele, avisa e volta o DNS ao automático. Link ou dono diferente: ignora.
/// `Ok(None)` = não há registro antigo (ou ele foi ignorado).
pub fn migrate_legacy() -> Result<Option<String>> {
    #[cfg(target_os = "linux")]
    return linux::migrate_legacy();
    #[cfg(target_os = "windows")]
    return windows::migrate_legacy();
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    Ok(None)
}

pub(crate) fn migrate_legacy_generic(
    known: &[String],
    revert: impl FnOnce(&DnsStateManifest) -> Result<()>,
    fallback: impl FnOnce() -> Result<()>,
) -> Result<Option<String>> {
    let legacy = match DnsStateManifest::load_legacy() {
        Ok(None) => return Ok(None),
        Ok(Some(m)) => m,
        Err(e) => {
            eprintln!("⚠️ Ignorando o registro antigo da proteção: {e:#}");
            return Ok(None);
        }
    };
    match legacy.validate_legacy(known) {
        Ok(()) => {
            println!("🔄 Restaurando a partir do registro antigo (beta)...");
            revert(&legacy)?;
            DnsStateManifest::remove_legacy();
            Ok(Some(
                "Configurações de DNS restauradas com sucesso (registro antigo migrado).".into(),
            ))
        }
        Err(motivo) => {
            eprintln!(
                "⚠️ O registro antigo da proteção não passou na validação ({motivo}); não vou usá-lo. Voltando o DNS ao padrão da rede."
            );
            DnsStateManifest::remove_legacy();
            fallback()?;
            Ok(Some(
                "DNS restaurado para o padrão da rede (o registro antigo era inválido).".into(),
            ))
        }
    }
}

pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    let _guard = crate::sysutil::guard_operation()?;
    #[cfg(target_os = "linux")]
    {
        linux::revert_dns(manifest)
    }

    #[cfg(target_os = "windows")]
    {
        windows::revert_dns(manifest)
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = manifest;
        anyhow::bail!("Sistema operacional não suportado nativamente pelo GuiaLar");
    }
}

/// Windows: reabre como administrador (UAC) para `subcomando`. `true` = já executado lá.
#[cfg(target_os = "windows")]
pub fn elevate_if_needed(subcomando: &str) -> Result<bool> {
    windows::elevate_if_needed(subcomando)
}

/// Windows: DNS + políticas dos navegadores numa **única** elevação (um só UAC).
/// Linux: só o DNS (o bloqueador é um passo separado, "Proteger navegadores").
pub fn apply_all() -> Result<()> {
    let _guard = crate::sysutil::guard_operation()?;
    match next_apply_step(configured_dns)? {
        ApplyStep::AlreadyActive => return Ok(()),
        ApplyStep::Switch(m) => return switch_mode(m),
        ApplyStep::Fresh => {}
    }
    #[cfg(target_os = "windows")]
    {
        windows::apply_all()
    }
    #[cfg(not(target_os = "windows"))]
    {
        apply_dns()
    }
}

/// O DNS em uso ainda é o filtro do GuiaLar? (para recuperar sem manifesto)
pub fn cloudflare_dns_active() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::cloudflare_dns_active()
    }
    #[cfg(target_os = "windows")]
    {
        windows::cloudflare_dns_active()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Reversão de emergência: sem manifesto utilizável, volta o DNS ao automático.
pub fn revert_dns_fallback() -> Result<()> {
    let _guard = crate::sysutil::guard_operation()?;
    #[cfg(target_os = "linux")]
    {
        linux::revert_dns_fallback()
    }
    #[cfg(target_os = "windows")]
    {
        windows::revert_dns_fallback()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("Sistema operacional não suportado nativamente pelo GuiaLar");
    }
}

/// Troca o filtro com a proteção ligada, sem perder o DNS original.
fn switch_mode(manifest: DnsStateManifest) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::switch_mode(manifest)
    }
    #[cfg(target_os = "windows")]
    {
        windows::switch_mode(manifest)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = manifest;
        anyhow::bail!("Sistema operacional não suportado nativamente pelo GuiaLar");
    }
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

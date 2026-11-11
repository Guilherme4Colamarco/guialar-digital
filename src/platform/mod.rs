//! Despacho de plataforma em tempo de compilação.

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

pub fn apply_dns() -> Result<()> {
    let _guard = crate::sysutil::guard_operation()?;
    // Idempotente: um segundo `apply` regravaria o manifesto com o DNS já da Cloudflare
    // como "original" e o `revert` nunca restauraria o DNS verdadeiro.
    if let Some(m) = DnsStateManifest::load()? {
        if plan_apply(Some(&m), crate::config::current_mode(), &configured_dns())
            == ApplyPlan::AlreadyActive
        {
            println!(
                "ℹ️  A proteção já está ativa. Use 'guialar revert' antes de aplicar de novo."
            );
            return Ok(());
        }
        return switch_mode(m); // reaplica o DNS do modo, mantendo o DNS original do manifesto
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
    if let Some(m) = DnsStateManifest::load()? {
        if plan_apply(Some(&m), crate::config::current_mode(), &configured_dns())
            == ApplyPlan::AlreadyActive
        {
            println!(
                "ℹ️  A proteção já está ativa. Use 'guialar revert' antes de aplicar de novo."
            );
            return Ok(());
        }
        return switch_mode(m);
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
}

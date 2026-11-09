//! Despacho de plataforma em tempo de compilação.

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::config::DnsStateManifest;
use anyhow::Result;

pub fn apply_dns() -> Result<()> {
    // Idempotente: um segundo `apply` regravaria o manifesto com o DNS já da Cloudflare
    // como "original" e o `revert` nunca restauraria o DNS verdadeiro.
    if DnsStateManifest::load()?.is_some() {
        println!("ℹ️  A proteção já está ativa. Use 'guialar revert' antes de aplicar de novo.");
        return Ok(());
    }
    // Windows: a instância elevada (UAC) reentra aqui; a trava seria dela mesma.
    #[cfg(target_os = "linux")]
    let _lock = crate::sysutil::OpLock::acquire()?;
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
    #[cfg(target_os = "linux")]
    let _lock = crate::sysutil::OpLock::acquire()?;
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

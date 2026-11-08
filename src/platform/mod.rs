//! Despacho de plataforma em tempo de compilação.

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use crate::config::DnsStateManifest;
use anyhow::Result;

pub fn apply_dns() -> Result<()> {
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

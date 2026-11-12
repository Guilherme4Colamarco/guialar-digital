//! Primitivas de arquivo para o que o processo elevado grava ou lê (ADR-028 rev. 2, ADR-030,
//! ADR-031).
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

/// `O_NOFOLLOW` (varia por arquitetura): abrir um link no último componente falha.
#[cfg(target_os = "linux")]
pub const O_NOFOLLOW: i32 = {
    // arm/aarch64 usam outro valor que o x86 e o resto (asm-generic).
    #[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
    {
        0o100000
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
    {
        0o400000
    }
};

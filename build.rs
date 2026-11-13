//! Embute ícone, VERSIONINFO e manifesto (DPI/ComCtl6) no .exe do Windows.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/guialar.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/guialar.ico");
    println!("cargo:rerun-if-env-changed=GUIALAR_WINDRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("guialar_res.o");
    let windres = std::env::var("GUIALAR_WINDRES").unwrap_or_else(|_| "llvm-windres".into());
    let status = Command::new(&windres)
        .args([
            "-O",
            "coff",
            "-I",
            "assets",
            "-i",
            "assets/guialar.rc",
            "-o",
        ])
        .arg(&out)
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        other => panic!(
            "falha ao compilar recursos do Windows com '{windres}' ({other:?}); defina GUIALAR_WINDRES"
        ),
    }
}

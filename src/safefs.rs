//! O processo elevado lê, apaga ou grava em áreas que um usuário comum controla, sem seguir
//! links (ADR-030, ADR-031).
//!
//! O estado do GuiaLar mora numa pasta do sistema (ADR-031). Sobra uma situação em que o
//! processo elevado ainda toca um caminho fora dela: no Windows, gravar o `policies.json` de
//! um Firefox. Um link simbólico (ou junção) nesse caminho faria o processo elevado ler,
//! gravar ou apagar o que não deve.
//!
//! Windows: recusa pontos de reparse (link simbólico e junção) em cada pasta e no arquivo; a
//! API padrão não tem operações relativas a um handle, então resta uma janela estreita entre
//! a checagem e o uso (documentada na ADR-030).
//! Linux: só sobra aqui o valor de `O_NOFOLLOW`, usado pela pasta de estado e pela trava. A
//! leitura sem seguir links na área do usuário saiu com a migração da beta (v0.4.0).

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use anyhow::Result;

/// `O_NOFOLLOW` (varia por arquitetura): abrir um link no último componente falha. O valor à
/// mão só existe para as arquiteturas conferidas; fora delas o build falha em vez de usar um
/// valor errado em silêncio.
#[cfg(target_os = "linux")]
pub const O_NOFOLLOW: i32 = {
    #[cfg(any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "powerpc",
        target_arch = "powerpc64"
    ))]
    {
        0o100000
    }
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "riscv64",
        target_arch = "riscv32"
    ))]
    {
        0o400000
    }
    #[cfg(not(any(
        target_arch = "aarch64",
        target_arch = "arm",
        target_arch = "powerpc",
        target_arch = "powerpc64",
        target_arch = "x86_64",
        target_arch = "x86",
        target_arch = "riscv64",
        target_arch = "riscv32"
    )))]
    {
        compile_error!("O_NOFOLLOW não conferido para esta arquitetura: confira em <asm/fcntl.h>")
    }
};

/// Dono esperado de um arquivo. No Windows não é conferido (a recusa é por ponto de reparse);
/// o tipo fica para a interface não mudar.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}

#[cfg(windows)]
pub fn caller_owner() -> Owner {
    Owner { uid: 0, gid: 0 }
}

#[cfg(windows)]
fn check_names(comps: &[&str], file: &str) -> Result<()> {
    for n in comps.iter().chain(std::iter::once(&file)) {
        if n.is_empty() || n.contains('/') || n.contains('\\') || n.contains('\0') || *n == ".." {
            anyhow::bail!("nome de pasta ou arquivo inválido: {n:?}");
        }
    }
    Ok(())
}

/// Lê `base/comps…/file`; `None` se não existe. Recusa link ou junção no caminho.
#[cfg(windows)]
pub fn read_file(
    base: &Path,
    comps: &[&str],
    file: &str,
    owner: &Owner,
) -> Result<Option<Vec<u8>>> {
    check_names(comps, file)?;
    imp::read_file(base, comps, file, owner)
}

/// Apaga `base/comps…/file` (só o nome; se for um link, remove o link e nunca o alvo).
#[cfg(windows)]
pub fn remove_file(base: &Path, comps: &[&str], file: &str, owner: &Owner) -> Result<()> {
    check_names(comps, file)?;
    imp::remove_file(base, comps, file, owner)
}

/// Grava `data` em `base/comps…/file` de forma atômica (temp + `rename`), criando as pastas
/// que faltam. Só Windows (o `policies.json` do Firefox). Recusa link/junção no caminho.
#[cfg(windows)]
pub fn write_file(base: &Path, comps: &[&str], file: &str, data: &[u8]) -> Result<()> {
    check_names(comps, file)?;
    imp::write_file(base, comps, file, data)
}

// ---------------------------------------------------------------------------------- Windows

#[cfg(windows)]
mod imp {
    use super::Owner;
    use anyhow::{bail, Context, Result};
    use std::io::Write;
    use std::os::windows::fs::MetadataExt;
    use std::path::{Path, PathBuf};

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    /// `Ok(true)` se existe e é comum; `Ok(false)` se não existe; erro se é link/junção.
    /// Confere o atributo de ponto de reparse: `is_symlink` não cobre toda junção.
    fn plain(path: &Path, want_dir: bool) -> Result<bool> {
        match std::fs::symlink_metadata(path) {
            Ok(m) => {
                if m.file_type().is_symlink()
                    || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                {
                    bail!(
                        "{} é um link simbólico ou junção; recuso atravessar",
                        path.display()
                    );
                }
                if want_dir != m.is_dir() {
                    bail!(
                        "{} não é {}; recuso",
                        path.display(),
                        if want_dir {
                            "uma pasta"
                        } else {
                            "um arquivo comum"
                        }
                    );
                }
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e).with_context(|| format!("não foi possível checar {}", path.display())),
        }
    }

    fn dir_chain(base: &Path, comps: &[&str], create: bool) -> Result<Option<PathBuf>> {
        let mut cur = base.to_path_buf();
        for c in comps {
            cur.push(c);
            if !plain(&cur, true)? {
                if !create {
                    return Ok(None);
                }
                std::fs::create_dir(&cur)
                    .with_context(|| format!("não foi possível criar {}", cur.display()))?;
                // confere de novo: nada pode ter virado junção entre criar e usar
                plain(&cur, true)?;
            }
        }
        Ok(Some(cur))
    }

    pub fn write_file(base: &Path, comps: &[&str], file: &str, data: &[u8]) -> Result<()> {
        let dir = dir_chain(base, comps, true)?.context("pasta ausente")?;
        let target = dir.join(file);
        plain(&target, false)?; // alvo existente como link/junção: recusa sem tocar
        let tmp = dir.join(format!(".{file}.{}.tmp", std::process::id()));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .with_context(|| format!("não foi possível criar {}", tmp.display()))?;
        let result = (|| -> Result<()> {
            f.write_all(data)?;
            f.flush()?;
            drop(f);
            // a pasta ainda é comum e o alvo ainda não virou link?
            dir_chain(base, comps, false)?;
            plain(&target, false)?;
            std::fs::rename(&tmp, &target)
                .with_context(|| format!("não foi possível gravar {}", target.display()))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }

    pub fn read_file(
        base: &Path,
        comps: &[&str],
        file: &str,
        _owner: &Owner,
    ) -> Result<Option<Vec<u8>>> {
        let Some(dir) = dir_chain(base, comps, false)? else {
            return Ok(None);
        };
        let target = dir.join(file);
        if !plain(&target, false)? {
            return Ok(None);
        }
        std::fs::read(&target)
            .map(Some)
            .with_context(|| format!("falha ao ler {}", target.display()))
    }

    pub fn remove_file(base: &Path, comps: &[&str], file: &str, _owner: &Owner) -> Result<()> {
        let Some(dir) = dir_chain(base, comps, false)? else {
            return Ok(());
        };
        let target = dir.join(file);
        match std::fs::symlink_metadata(&target) {
            Ok(_) => std::fs::remove_file(&target)
                .with_context(|| format!("não foi possível remover {}", target.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => {
                Err(e).with_context(|| format!("não foi possível checar {}", target.display()))
            }
        }
    }
}

/// Cria uma junção (`mklink /J`, não pede administrador) de `link` para `alvo`.
#[cfg(all(test, windows))]
pub(crate) fn junction(link: &Path, alvo: &Path) {
    let st = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(alvo)
        .output()
        .expect("mklink");
    assert!(st.status.success(), "mklink /J falhou: {st:?}");
}

#[cfg(all(test, windows))]
mod tests_win {
    use super::*;
    use crate::testenv::Sandbox;

    #[test]
    fn grava_le_e_remove_sem_junção() {
        let s = Sandbox::new("safefs-win-ok");
        let o = caller_owner();
        write_file(s.dir(), &["distribution"], "policies.json", b"{\"a\":1}").unwrap();
        let lido = read_file(s.dir(), &["distribution"], "policies.json", &o).unwrap();
        assert_eq!(lido.as_deref(), Some(&b"{\"a\":1}"[..]));
        write_file(s.dir(), &["distribution"], "policies.json", b"{}").unwrap(); // regrava
        let nomes: Vec<_> = std::fs::read_dir(s.dir().join("distribution"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(nomes, ["policies.json"], "sem .tmp esquecido");
        remove_file(s.dir(), &["distribution"], "policies.json", &o).unwrap();
        assert!(read_file(s.dir(), &["distribution"], "policies.json", &o)
            .unwrap()
            .is_none());
    }

    #[test]
    fn pasta_como_juncao_para_fora_e_recusada_sem_tocar_o_alvo() {
        let s = Sandbox::new("safefs-win-junction");
        let o = caller_owner();
        let fora = s.dir().join("FORA");
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("policies.json"), "do-alvo").unwrap();
        junction(&s.dir().join("distribution"), &fora); // distribution -> FORA
        for r in [
            write_file(s.dir(), &["distribution"], "policies.json", b"x").map(|_| ()),
            read_file(s.dir(), &["distribution"], "policies.json", &o).map(|_| ()),
            remove_file(s.dir(), &["distribution"], "policies.json", &o),
        ] {
            let e = r.unwrap_err();
            assert!(format!("{e:#}").contains("junção"), "{e:#}");
        }
        assert_eq!(
            std::fs::read_to_string(fora.join("policies.json")).unwrap(),
            "do-alvo"
        );
        let nomes: Vec<_> = std::fs::read_dir(&fora)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(nomes, ["policies.json"], "nada criado no alvo");
    }
}

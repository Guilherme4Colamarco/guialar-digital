//! O processo elevado lê, apaga ou grava em áreas que um usuário comum controla, sem seguir
//! links (ADR-030, ADR-031).
//!
//! O estado do GuiaLar mora numa pasta do sistema (ADR-031). Sobram duas situações em que o
//! processo elevado ainda toca a área de um usuário: **migrar** o manifesto antigo da beta
//! (`~/.config/guialar`, `%LOCALAPPDATA%\GuiaLar`) e, no Windows, gravar o `policies.json` de
//! um Firefox. Nas duas o caminho está sob controle de uma conta sem privilégio, e um link
//! simbólico (ou junção) faria o processo elevado ler, gravar ou apagar o que não deve.
//!
//! Linux: cada pasta é aberta com `O_NOFOLLOW|O_DIRECTORY` a partir do descritor da anterior;
//! recusa link e pasta ou arquivo de outro dono; o arquivo é aberto com `O_NOFOLLOW`.
//! Windows: recusa pontos de reparse (link simbólico e junção) em cada pasta e no arquivo; a
//! API padrão não tem operações relativas a um handle, então resta uma janela estreita entre
//! a checagem e o uso (documentada na ADR-030).
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

use std::path::Path;

use anyhow::Result;

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

/// Dono que os arquivos de uma área de usuário devem ter: quem chamou `sudo`/`pkexec` (ou o
/// próprio processo, quando não está elevado). No Windows não é usado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}

/// Dono esperado de um arquivo na área do usuário que chamou.
pub fn caller_owner() -> Owner {
    #[cfg(unix)]
    {
        if let Some((_, uid, gid, _)) = crate::sysutil::caller() {
            return Owner { uid, gid };
        }
        if let Some((uid, gid)) = crate::sysutil::current_ids() {
            return Owner { uid, gid };
        }
    }
    Owner { uid: 0, gid: 0 }
}

fn check_names(comps: &[&str], file: &str) -> Result<()> {
    for n in comps.iter().chain(std::iter::once(&file)) {
        if n.is_empty() || n.contains('/') || n.contains('\\') || n.contains('\0') || *n == ".." {
            anyhow::bail!("nome de pasta ou arquivo inválido: {n:?}");
        }
    }
    Ok(())
}

/// Lê `base/comps…/file`; `None` se não existe. Recusa link e dono diferente.
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

// ------------------------------------------------------------------------------------ Linux

#[cfg(target_os = "linux")]
mod sys {
    use std::ffi::{c_char, c_int};

    pub const O_RDONLY: c_int = 0;
    pub const O_NONBLOCK: c_int = 0o4000;
    pub const O_CLOEXEC: c_int = 0o2000000;
    #[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
    pub const O_DIRECTORY: c_int = 0o40000;
    #[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
    pub const O_DIRECTORY: c_int = 0o200000;
    pub const O_NOFOLLOW: c_int = super::O_NOFOLLOW;
    pub const AT_FDCWD: c_int = -100;

    extern "C" {
        pub fn openat(dirfd: c_int, pathname: *const c_char, flags: c_int, ...) -> c_int;
        pub fn unlinkat(dirfd: c_int, pathname: *const c_char, flags: c_int) -> c_int;
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::{sys, Owner};
    use anyhow::{bail, Context, Result};
    use std::ffi::CString;
    use std::fs::File;
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    fn open_at(dirfd: i32, name: &str, flags: i32) -> std::io::Result<OwnedFd> {
        let c = CString::new(name)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        // SAFETY: `c` é uma string C válida; os argumentos seguem o contrato de openat(2).
        let fd = unsafe { sys::openat(dirfd, c.as_ptr(), flags | sys::O_CLOEXEC, 0u32) };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            // SAFETY: `fd` acabou de ser aberto e é só nosso.
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }

    fn is_link_error(e: &std::io::Error) -> bool {
        // `O_NOFOLLOW` num link dá ELOOP; com `O_DIRECTORY` o kernel pode devolver ENOTDIR.
        matches!(e.raw_os_error(), Some(40) | Some(20))
    }

    fn check_owner(fd: &OwnedFd, owner: &Owner, what: &str) -> Result<()> {
        let md = File::from(fd.try_clone()?).metadata()?;
        if md.uid() != owner.uid {
            bail!(
                "{what} pertence a outro usuário (dono {}, esperado {}); recuso",
                md.uid(),
                owner.uid
            );
        }
        Ok(())
    }

    /// Desce `comps` a partir de `base` (caminho confiável: passwd/ambiente) sem seguir links.
    fn walk(base: &Path, comps: &[&str], owner: &Owner) -> Result<Option<OwnedFd>> {
        let c = CString::new(base.as_os_str().as_encoded_bytes()).context("caminho inválido")?;
        // SAFETY: string C válida.
        let fd = unsafe {
            sys::openat(
                sys::AT_FDCWD,
                c.as_ptr(),
                sys::O_RDONLY | sys::O_DIRECTORY | sys::O_CLOEXEC,
            )
        };
        if fd < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(e).with_context(|| format!("não foi possível abrir {}", base.display()));
        }
        // SAFETY: `fd` recém-aberto.
        let mut cur = unsafe { OwnedFd::from_raw_fd(fd) };
        check_owner(&cur, owner, &base.display().to_string())?;
        let mut path = base.to_path_buf();
        for name in comps {
            path.push(name);
            let flags = sys::O_RDONLY | sys::O_DIRECTORY | sys::O_NOFOLLOW;
            let next = match open_at(cur.as_raw_fd(), name, flags) {
                Ok(fd) => fd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) if is_link_error(&e) => bail!(
                    "{} é um link ou não é uma pasta; recuso seguir",
                    path.display()
                ),
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("não foi possível abrir {}", path.display()))
                }
            };
            check_owner(&next, owner, &path.display().to_string())?;
            cur = next;
        }
        Ok(Some(cur))
    }

    pub fn read_file(
        base: &Path,
        comps: &[&str],
        file: &str,
        owner: &Owner,
    ) -> Result<Option<Vec<u8>>> {
        let Some(dir) = walk(base, comps, owner)? else {
            return Ok(None);
        };
        let target = comps
            .iter()
            .fold(base.to_path_buf(), |p, c| p.join(c))
            .join(file);
        let fd = match open_at(
            dir.as_raw_fd(),
            file,
            sys::O_RDONLY | sys::O_NOFOLLOW | sys::O_NONBLOCK,
        ) {
            Ok(fd) => fd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) if is_link_error(&e) => bail!(
                "{} é um link; recuso seguir (o alvo não foi tocado)",
                target.display()
            ),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("não foi possível abrir {}", target.display()))
            }
        };
        let mut f = File::from(fd);
        let md = f.metadata()?;
        if !md.is_file() {
            bail!("{} não é um arquivo comum; recuso", target.display());
        }
        if md.uid() != owner.uid {
            bail!("{} pertence a outro usuário; recuso", target.display());
        }
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)
            .with_context(|| format!("falha ao ler {}", target.display()))?;
        Ok(Some(buf))
    }

    pub fn remove_file(base: &Path, comps: &[&str], file: &str, owner: &Owner) -> Result<()> {
        let Some(dir) = walk(base, comps, owner)? else {
            return Ok(());
        };
        let c = CString::new(file)?;
        // SAFETY: descritor da pasta e string C válidos. `unlinkat` não segue o link final.
        let r = unsafe { sys::unlinkat(dir.as_raw_fd(), c.as_ptr(), 0) };
        if r < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(e).with_context(|| format!("não foi possível remover {file}"));
            }
        }
        Ok(())
    }
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

// ------------------------------------------------------------- outros Unix (sem uso real)

#[cfg(all(unix, not(target_os = "linux")))]
mod imp {
    use super::Owner;
    use anyhow::{bail, Result};
    use std::path::Path;

    pub fn read_file(_: &Path, _: &[&str], _: &str, _: &Owner) -> Result<Option<Vec<u8>>> {
        bail!("plataforma sem suporte")
    }
    pub fn remove_file(_: &Path, _: &[&str], _: &str, _: &Owner) -> Result<()> {
        bail!("plataforma sem suporte")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::testenv::Sandbox;
    use std::os::unix::fs::symlink;

    fn legado(s: &Sandbox, conteudo: &str) -> std::path::PathBuf {
        let dir = s.dir().join(".config/guialar");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), conteudo).unwrap();
        dir
    }

    #[test]
    fn le_e_remove_o_arquivo_do_usuario() {
        let s = Sandbox::new("safefs-ok");
        let o = caller_owner();
        legado(&s, "{\"a\":1}");
        let lido = read_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap();
        assert_eq!(lido.as_deref(), Some(&b"{\"a\":1}"[..]));
        remove_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap();
        assert!(
            read_file(s.dir(), &[".config", "guialar"], "state.json", &o)
                .unwrap()
                .is_none()
        );
        // o que não existe não é erro
        remove_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap();
        remove_file(s.dir(), &["nao", "existe"], "state.json", &o).unwrap();
        assert!(read_file(s.dir(), &["nao", "existe"], "state.json", &o)
            .unwrap()
            .is_none());
    }

    #[test]
    fn pasta_como_link_para_fora_e_recusada_sem_tocar_o_alvo() {
        let s = Sandbox::new("safefs-dirlink");
        let fora = s.dir().join("FORA");
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("state.json"), "do-alvo").unwrap();
        let cfg = s.dir().join(".config");
        std::fs::create_dir_all(&cfg).unwrap();
        symlink(&fora, cfg.join("guialar")).unwrap(); // ~/.config/guialar -> FORA
        let o = caller_owner();
        let e = read_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap_err();
        assert!(format!("{e:#}").contains("link"), "{e:#}");
        let e = remove_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap_err();
        assert!(format!("{e:#}").contains("link"), "{e:#}");
        assert_eq!(
            std::fs::read_to_string(fora.join("state.json")).unwrap(),
            "do-alvo",
            "o alvo do link não foi lido nem apagado"
        );
        // `.config` como link
        std::fs::remove_file(cfg.join("guialar")).unwrap();
        std::fs::remove_dir(&cfg).unwrap();
        symlink(&fora, &cfg).unwrap();
        assert!(read_file(s.dir(), &[".config", "guialar"], "state.json", &o).is_err());
    }

    #[test]
    fn pasta_ou_arquivo_de_outro_dono_e_recusado() {
        let s = Sandbox::new("safefs-owner");
        let o = caller_owner();
        let dir = legado(&s, "x");
        let outro = Owner {
            uid: o.uid + 1,
            gid: o.gid,
        };
        let e = read_file(s.dir(), &[".config", "guialar"], "state.json", &outro).unwrap_err();
        assert!(format!("{e:#}").contains("outro usuário"), "{e:#}");
        assert!(remove_file(s.dir(), &[".config", "guialar"], "state.json", &outro).is_err());
        assert!(dir.join("state.json").exists(), "nada foi apagado");
    }

    #[test]
    fn arquivo_alvo_como_link_e_recusado_e_remover_tira_so_o_link() {
        let s = Sandbox::new("safefs-filelink");
        let o = caller_owner();
        let dir = s.dir().join(".config/guialar");
        std::fs::create_dir_all(&dir).unwrap();
        let alvo = s.dir().join("ALVO-FORA");
        std::fs::write(&alvo, "conteudo-original").unwrap();
        symlink(&alvo, dir.join("state.json")).unwrap(); // state.json -> ALVO-FORA
        let e = read_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap_err();
        assert!(format!("{e:#}").contains("link"), "{e:#}");
        remove_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap();
        assert!(!dir.join("state.json").exists());
        assert_eq!(std::fs::read_to_string(&alvo).unwrap(), "conteudo-original");
    }

    #[test]
    fn nao_e_arquivo_comum_e_recusado_e_nomes_invalidos_tambem() {
        let s = Sandbox::new("safefs-kind");
        let o = caller_owner();
        let dir = s.dir().join(".config/guialar");
        std::fs::create_dir_all(dir.join("state.json")).unwrap(); // pasta no lugar do arquivo
        let e = read_file(s.dir(), &[".config", "guialar"], "state.json", &o).unwrap_err();
        assert!(format!("{e:#}").contains("não é um arquivo comum"), "{e:#}");
        for (c, f) in [
            (vec!["a/b"], "x"),
            (vec![".."], "x"),
            (vec!["a"], "../x"),
            (vec![""], "x"),
            (vec!["a"], ""),
        ] {
            assert!(read_file(s.dir(), &c, f, &o).is_err(), "{c:?} {f:?}");
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

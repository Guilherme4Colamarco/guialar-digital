//! Estado do GuiaLar numa pasta do sistema, não na área do usuário (ADR-031).
//!
//! - Linux: `/var/lib/guialar/` (`root:root 0755`), arquivos `0644`.
//! - Windows: `%ProgramData%\GuiaLar\` (via `SHGetKnownFolderPath`), criada já com DACL
//!   protegida (Administradores e SYSTEM com controle total, Usuários só leitura) e dono
//!   Administradores.
//!
//! Só o processo elevado grava; o do usuário só lê. Antes de confiar no que lê, confere o dono
//! (root no Linux; Administradores ou SYSTEM no Windows) e ignora, avisando, o que não for.
#![cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

pub const STATE_FILE: &str = "state.json";

/// Pasta de estado do sistema. Nos testes, uma pasta dentro do `Sandbox`.
pub fn state_dir() -> Result<PathBuf> {
    #[cfg(test)]
    if let Some(d) = crate::testenv::state_dir() {
        return Ok(d);
    }
    // Só em build de depuração (testes de contrato que rodam o binário): pasta de estado de
    // mentira, sem tocar no `/var/lib` real, como o `GUIALAR_TEST_DNS` faz com o DNS.
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var_os("GUIALAR_TEST_STATE_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    imp::system_dir()
}

pub fn state_file() -> Result<PathBuf> {
    Ok(state_dir()?.join(STATE_FILE))
}

/// Cria (ou valida) a pasta de estado. Só o processo elevado a chama.
pub fn ensure_dir() -> Result<PathBuf> {
    imp::ensure_dir()
}

/// O arquivo ou a pasta têm dono confiável e não são link/ponto de reparse?
pub fn is_trusted(path: &Path) -> bool {
    imp::is_trusted(path)
}

/// Grava o manifesto (temp + `rename`) na pasta do sistema.
pub fn write_state(data: &[u8]) -> Result<()> {
    let dir = ensure_dir()?;
    imp::write_atomic(&dir, STATE_FILE, data)
}

/// Lê o manifesto; `None` se não existe ou se pasta/arquivo não são de dono confiável (nesse
/// caso avisa e ignora).
pub fn read_state() -> Result<Option<Vec<u8>>> {
    let dir = state_dir()?;
    if std::fs::symlink_metadata(&dir).is_err() {
        return Ok(None);
    }
    let file = dir.join(STATE_FILE);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    if !is_trusted(&dir) || !is_trusted(&file) {
        eprintln!(
            "⚠️ Ignorando {}: o dono não é o do sistema (o registro da proteção não foi usado).",
            file.display()
        );
        return Ok(None);
    }
    std::fs::read(&file)
        .map(Some)
        .with_context(|| format!("Falha ao ler manifesto em {file:?}"))
}

/// Apaga o manifesto do sistema (só o nome do arquivo).
pub fn remove_state() -> Result<()> {
    let file = state_file()?;
    match std::fs::symlink_metadata(&file) {
        Ok(_) => std::fs::remove_file(&file)
            .with_context(|| format!("Falha ao remover manifesto {file:?}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("Falha ao checar manifesto {file:?}")),
    }
}

// ------------------------------------------------------------------------------------ Linux

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};

    pub fn system_dir() -> Result<PathBuf> {
        Ok(PathBuf::from("/var/lib/guialar"))
    }

    /// UID que deve ser dono do estado: root. Nos testes, o usuário que roda o teste (ou o
    /// que o teste fingir ser).
    fn trusted_uid() -> u32 {
        #[cfg(test)]
        if crate::testenv::sandboxed() {
            return crate::testenv::trusted_uid()
                .or_else(|| crate::sysutil::current_ids().map(|i| i.0))
                .unwrap_or(0);
        }
        // pasta de mentira dos testes de contrato (só depuração): dono é quem roda o teste
        #[cfg(debug_assertions)]
        if std::env::var_os("GUIALAR_TEST_STATE_DIR").is_some_and(|v| !v.is_empty()) {
            return crate::sysutil::current_ids().map(|i| i.0).unwrap_or(0);
        }
        0
    }

    /// Pasta ou arquivo comum, do dono confiável e sem escrita para grupo/outros.
    pub fn is_trusted(path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|m| {
            let ft = m.file_type();
            !ft.is_symlink()
                && (ft.is_dir() || ft.is_file())
                && m.uid() == trusted_uid()
                && m.mode() & 0o022 == 0
        })
    }

    pub fn ensure_dir() -> Result<PathBuf> {
        let dir = state_dir()?;
        match std::fs::symlink_metadata(&dir) {
            Ok(_) => {
                if !is_trusted(&dir) {
                    bail!(
                        "{} não é uma pasta segura do sistema (link, dono ou permissões diferentes); recuso gravar o estado ali",
                        dir.display()
                    );
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::fs::DirBuilder::new()
                    .mode(0o755)
                    .create(&dir)
                    .with_context(|| format!("não foi possível criar {}", dir.display()))?;
                // a umask do root pode ter fechado a pasta: o usuário precisa ler o estado
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
                    .with_context(|| format!("permissões de {}", dir.display()))?;
            }
            Err(e) => return Err(e).context("não foi possível checar a pasta de estado"),
        }
        Ok(dir)
    }

    pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<()> {
        let mut tag = [0u8; 6];
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = std::io::Read::read_exact(&mut f, &mut tag);
        }
        let tmp = dir.join(format!(
            ".{name}.{}.{}.tmp",
            std::process::id(),
            tag.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        let target = dir.join(name);
        let result = (|| -> Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o644)
                .custom_flags(crate::safefs::O_NOFOLLOW)
                .open(&tmp)
                .with_context(|| format!("Falha ao criar arquivo temporário {tmp:?}"))?;
            f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
            f.write_all(data)?;
            f.flush()?;
            f.sync_all().ok();
            std::fs::rename(&tmp, &target)
                .with_context(|| format!("Falha ao renomear {tmp:?} para {target:?}"))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

// ---------------------------------------------------------------------------------- Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::win32;
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    /// Administradores e SYSTEM.
    const TRUSTED_SIDS: [&str; 2] = ["S-1-5-32-544", "S-1-5-18"];
    /// DACL protegida (sem herança do pai): Administradores e SYSTEM controle total, Usuários
    /// só leitura; dono e grupo definidos no mesmo descritor da criação.
    const SDDL: &str = "O:BAG:SYD:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)(A;OICI;FR;;;BU)";

    fn wide(p: &std::ffi::OsStr) -> Vec<u16> {
        p.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn from_wide_ptr(p: *const u16) -> String {
        let mut n = 0;
        // SAFETY: o Windows devolve uma string terminada em NUL.
        unsafe {
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    }

    /// `%ProgramData%` pelo `SHGetKnownFolderPath`, nunca pela variável de ambiente.
    pub fn system_dir() -> Result<PathBuf> {
        let mut p: *mut u16 = std::ptr::null_mut();
        // SAFETY: GUID e ponteiro de saída válidos; liberamos com CoTaskMemFree.
        let hr = unsafe {
            win32::SHGetKnownFolderPath(
                &win32::FOLDERID_PROGRAMDATA,
                0,
                std::ptr::null_mut(),
                &mut p,
            )
        };
        if hr != 0 || p.is_null() {
            bail!("não foi possível localizar a pasta ProgramData (HRESULT {hr:#x})");
        }
        let s = from_wide_ptr(p);
        // SAFETY: `p` veio do SHGetKnownFolderPath.
        unsafe { win32::CoTaskMemFree(p.cast()) };
        Ok(PathBuf::from(s).join("GuiaLar"))
    }

    fn owner_sid(path: &Path) -> Option<String> {
        let w = wide(path.as_os_str());
        let mut owner: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: nome terminado em NUL e ponteiros de saída válidos.
        let r = unsafe {
            win32::GetNamedSecurityInfoW(
                w.as_ptr(),
                win32::SE_FILE_OBJECT,
                win32::OWNER_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        if r != 0 {
            return None;
        }
        let mut s: *mut u16 = std::ptr::null_mut();
        // SAFETY: `owner` aponta para dentro de `sd` (válido até o LocalFree abaixo).
        let ok = unsafe { win32::ConvertSidToStringSidW(owner, &mut s) };
        let out = (ok != 0 && !s.is_null()).then(|| from_wide_ptr(s));
        // SAFETY: ambos vieram do Windows e são liberados uma vez.
        unsafe {
            if !s.is_null() {
                win32::LocalFree(s.cast());
            }
            win32::LocalFree(sd);
        }
        out
    }

    /// Não é link/ponto de reparse e o dono é Administradores ou SYSTEM. Nos testes (que usam
    /// uma pasta do usuário) só vale a checagem de reparse.
    pub fn is_trusted(path: &Path) -> bool {
        let Ok(m) = std::fs::symlink_metadata(path) else {
            return false;
        };
        if m.file_type().is_symlink() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
        #[cfg(test)]
        if crate::testenv::sandboxed() {
            return true;
        }
        owner_sid(path).is_some_and(|s| TRUSTED_SIDS.contains(&s.as_str()))
    }

    /// Cria a pasta com a DACL protegida e o dono Administradores num passo só.
    fn create_protected(dir: &Path) -> Result<()> {
        #[cfg(test)]
        if crate::testenv::sandboxed() {
            return std::fs::create_dir(dir).map_err(Into::into);
        }
        let sddl: Vec<u16> = SDDL.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: SDDL terminado em NUL; `sd` é liberado com LocalFree.
        let ok = unsafe {
            win32::ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                win32::SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            bail!("não foi possível montar a ACL da pasta de estado");
        }
        let sa = win32::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<win32::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let w = wide(dir.as_os_str());
        // SAFETY: caminho terminado em NUL e atributos válidos durante a chamada.
        let r = unsafe { win32::CreateDirectoryW(w.as_ptr(), &sa) };
        let err = std::io::Error::last_os_error();
        // SAFETY: `sd` veio da conversão acima.
        unsafe { win32::LocalFree(sd) };
        if r == 0 {
            return Err(err).with_context(|| format!("não foi possível criar {}", dir.display()));
        }
        Ok(())
    }

    pub fn ensure_dir() -> Result<PathBuf> {
        let dir = state_dir()?;
        for _ in 0..2 {
            match std::fs::symlink_metadata(&dir) {
                Ok(m) if m.is_dir() && is_trusted(&dir) => return Ok(dir),
                Ok(_) => {
                    // pasta que não é nossa (outro dono, link ou reparse): sai do caminho,
                    // sem seguir a junção, e a certa é recriada. Recusar para sempre deixaria
                    // a proteção bloqueada por qualquer conta comum.
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let aside = dir.with_file_name(format!("GuiaLar.recusada-{ts}"));
                    std::fs::rename(&dir, &aside).with_context(|| {
                        format!("não foi possível afastar {} (não é nossa)", dir.display())
                    })?;
                    eprintln!(
                        "⚠️ {} não era do sistema; renomeada para {} e recriada.",
                        dir.display(),
                        aside.display()
                    );
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if create_protected(&dir).is_ok() {
                        return Ok(dir);
                    }
                    // alguém criou no meio: o laço confere de novo
                }
                Err(e) => return Err(e).context("não foi possível checar a pasta de estado"),
            }
        }
        bail!("não foi possível preparar {}", dir.display())
    }

    pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<()> {
        let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
        let target = dir.join(name);
        let result = (|| -> Result<()> {
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .with_context(|| format!("Falha ao criar arquivo temporário {tmp:?}"))?;
            f.write_all(data)?;
            f.flush()?;
            drop(f);
            std::fs::rename(&tmp, &target)
                .with_context(|| format!("Falha ao renomear {tmp:?} para {target:?}"))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
mod imp {
    use super::*;
    pub fn system_dir() -> Result<PathBuf> {
        bail!("plataforma sem suporte")
    }
    pub fn is_trusted(_: &Path) -> bool {
        false
    }
    pub fn ensure_dir() -> Result<PathBuf> {
        bail!("plataforma sem suporte")
    }
    pub fn write_atomic(_: &Path, _: &str, _: &[u8]) -> Result<()> {
        bail!("plataforma sem suporte")
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::testenv::Sandbox;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    fn modo(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn estado_so_na_pasta_do_sistema_com_0755_e_0644() {
        let s = Sandbox::new("statedir-ok");
        let dir = state_dir().unwrap();
        assert!(dir.starts_with(s.dir().join("sistema")));
        assert!(!dir.exists(), "a produção é quem cria a pasta");
        write_state(b"{\"a\":1}").unwrap();
        assert_eq!(modo(&dir), 0o755);
        assert_eq!(modo(&dir.join(STATE_FILE)), 0o644);
        assert_eq!(std::fs::read(dir.join(STATE_FILE)).unwrap(), b"{\"a\":1}");
        // nada na área do usuário
        assert!(!s.dir().join(".config").exists());
        assert!(!s.dir().join("tmp").join("state.json").exists());
        // sem arquivo temporário esquecido
        let nomes: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(nomes, [STATE_FILE]);
        // ler (sem elevar) e remover
        assert_eq!(read_state().unwrap().as_deref(), Some(&b"{\"a\":1}"[..]));
        remove_state().unwrap();
        assert!(read_state().unwrap().is_none());
        remove_state().unwrap(); // idempotente
    }

    #[test]
    fn a_umask_fechada_do_root_nao_esconde_o_estado_do_usuario() {
        let s = Sandbox::new("statedir-umask");
        // umask 077 do processo: a pasta e o arquivo ainda saem 0755/0644
        extern "C" {
            fn umask(mask: u32) -> u32;
        }
        // SAFETY: só troca e restaura a máscara do processo.
        let antiga = unsafe { umask(0o077) };
        let r = write_state(b"{}");
        // SAFETY: restaura a máscara.
        unsafe { umask(antiga) };
        r.unwrap();
        let dir = state_dir().unwrap();
        assert_eq!(modo(&dir), 0o755);
        assert_eq!(modo(&dir.join(STATE_FILE)), 0o644);
        drop(s);
    }

    #[test]
    fn pasta_preexistente_como_link_de_outro_dono_ou_larga_e_recusada() {
        let s = Sandbox::new("statedir-recusa");
        let dir = state_dir().unwrap();
        let fora = s.dir().join("FORA");
        std::fs::create_dir_all(&fora).unwrap();
        // link
        symlink(&fora, &dir).unwrap();
        let e = write_state(b"{}").unwrap_err();
        assert!(format!("{e:#}").contains("não é uma pasta segura"), "{e:#}");
        assert!(ensure_dir().is_err());
        assert!(
            std::fs::read_dir(&fora).unwrap().next().is_none(),
            "nada foi criado no alvo"
        );
        std::fs::remove_file(&dir).unwrap();
        // escrita para outros
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(write_state(b"{}").is_err());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        // dono diferente do esperado (o teste finge que o dono confiável é outro uid)
        let outro = std::fs::metadata(&dir).unwrap().uid() + 1;
        crate::testenv::set_trusted_uid(Some(outro));
        let e = write_state(b"{}").unwrap_err();
        crate::testenv::set_trusted_uid(None);
        assert!(format!("{e:#}").contains("não é uma pasta segura"), "{e:#}");
        assert!(!dir.join(STATE_FILE).exists());
    }

    #[test]
    fn estado_de_outro_dono_ou_link_e_ignorado_como_sem_estado() {
        let s = Sandbox::new("statedir-ignora");
        write_state(b"{\"valido\":true}").unwrap();
        assert!(read_state().unwrap().is_some());
        // o dono do arquivo não é o confiável: ignora, sem ler
        let uid = std::fs::metadata(state_dir().unwrap()).unwrap().uid();
        crate::testenv::set_trusted_uid(Some(uid + 1));
        assert!(read_state().unwrap().is_none());
        crate::testenv::set_trusted_uid(None);
        // arquivo trocado por um link: ignora e não lê o alvo
        let dir = state_dir().unwrap();
        let alvo = s.dir().join("ALVO");
        std::fs::write(&alvo, "{\"forjado\":true}").unwrap();
        std::fs::remove_file(dir.join(STATE_FILE)).unwrap();
        symlink(&alvo, dir.join(STATE_FILE)).unwrap();
        assert!(read_state().unwrap().is_none());
        assert_eq!(
            std::fs::read_to_string(&alvo).unwrap(),
            "{\"forjado\":true}"
        );
        // arquivo com escrita para outros: ignorado
        std::fs::remove_file(dir.join(STATE_FILE)).unwrap();
        std::fs::write(dir.join(STATE_FILE), "{}").unwrap();
        std::fs::set_permissions(dir.join(STATE_FILE), std::fs::Permissions::from_mode(0o666))
            .unwrap();
        assert!(read_state().unwrap().is_none());
    }
}

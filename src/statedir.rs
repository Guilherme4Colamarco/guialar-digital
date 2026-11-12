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

/// Apaga um arquivo AGORA: no Windows desvincula o nome na hora (`FileDispositionInfoEx` com
/// semântica POSIX), mesmo que outro processo o tenha aberto com `FILE_SHARE_DELETE`. Um
/// arquivo aberto SEM esse compartilhamento não pode ser apagado nem renomeado por ninguém
/// enquanto o outro processo o segurar (medido na win11): o erro sobe.
pub fn unlink_now(path: &Path) -> std::io::Result<()> {
    imp::unlink_now(path)
}

/// Nomes que o GuiaLar grava na pasta de estado e de que o apply depende: se um intruso os
/// ocupa e não dá para apagar, o apply tem de parar com um erro claro. Qualquer outro nome
/// alheio que não saia só é avisado (a pasta já está fechada; o arquivo nunca é lido).
#[cfg(any(windows, test))]
fn is_core_name(name: &str) -> bool {
    name == STATE_FILE || name == "op.lock"
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

    pub fn unlink_now(path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
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

    pub fn owner_sid(path: &Path) -> Option<String> {
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

    /// Só para testes de ACL real: troca o dono de `path` (SDDL tipo `O:BU`).
    #[cfg(test)]
    pub fn test_set_owner(path: &Path, owner_sddl: &str) -> Result<()> {
        enable_privilege("SeTakeOwnershipPrivilege");
        enable_privilege("SeRestorePrivilege");
        let sd = security_descriptor_from(owner_sddl)?;
        let mut owner: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut defaulted = 0i32;
        // SAFETY: `sd` válido; ponteiro de saída interno a `sd`.
        let ok = unsafe { win32::GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) };
        let w = wide(path.as_os_str());
        // SAFETY: caminho terminado em NUL; `owner` pertence a `sd`.
        let r = unsafe {
            win32::SetNamedSecurityInfoW(
                w.as_ptr(),
                win32::SE_FILE_OBJECT,
                win32::OWNER_SECURITY_INFORMATION,
                owner,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // SAFETY: `sd` veio da conversão.
        unsafe { win32::LocalFree(sd) };
        if ok == 0 || r != 0 {
            bail!(
                "não foi possível trocar o dono de {} (erro {r})",
                path.display()
            );
        }
        Ok(())
    }

    /// Descritor de segurança do GuiaLar a partir do SDDL (liberar com `LocalFree`).
    fn security_descriptor() -> Result<*mut std::ffi::c_void> {
        security_descriptor_from(SDDL)
    }

    fn security_descriptor_from(sddl: &str) -> Result<*mut std::ffi::c_void> {
        let sddl: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: SDDL terminado em NUL; `sd` é liberado por quem chama.
        let ok = unsafe {
            win32::ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                win32::SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || sd.is_null() {
            bail!("não foi possível montar a ACL da pasta de estado");
        }
        Ok(sd)
    }

    /// Cria a pasta com a DACL protegida e o dono Administradores num passo só.
    fn create_protected(dir: &Path) -> Result<()> {
        #[cfg(test)]
        if crate::testenv::sandboxed() {
            return std::fs::create_dir(dir).map_err(Into::into);
        }
        let sd = security_descriptor()?;
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

    /// 128 bits aleatórios em hexadecimal (`RtlGenRandom`).
    fn random_hex() -> Result<String> {
        let mut b = [0u8; 16];
        // SAFETY: buffer válido de 16 bytes.
        if unsafe { win32::RtlGenRandom(b.as_mut_ptr(), 16) } == 0 {
            bail!("não foi possível gerar bytes aleatórios");
        }
        Ok(b.iter().map(|x| format!("{x:02x}")).collect())
    }

    /// Liga um privilégio do token (best effort: administradores têm, mas desligados).
    fn enable_privilege(name: &str) {
        let n: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut token: win32::HANDLE = std::ptr::null_mut();
        // SAFETY: ponteiros de saída válidos; o handle é fechado abaixo.
        unsafe {
            if win32::OpenProcessToken(
                win32::GetCurrentProcess(),
                win32::TOKEN_ADJUST_PRIVILEGES | win32::TOKEN_QUERY,
                &mut token,
            ) == 0
            {
                return;
            }
            let mut luid = win32::LUID {
                LowPart: 0,
                HighPart: 0,
            };
            if win32::LookupPrivilegeValueW(std::ptr::null(), n.as_ptr(), &mut luid) != 0 {
                let tp = win32::TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Luid: luid,
                    Attributes: win32::SE_PRIVILEGE_ENABLED,
                };
                win32::AdjustTokenPrivileges(
                    token,
                    0,
                    &tp,
                    0,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            }
            win32::CloseHandle(token);
        }
    }

    /// Handle que fecha sozinho.
    struct Handle(win32::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: o handle veio de `CreateFileW` e é fechado uma vez.
            unsafe { win32::CloseHandle(self.0) };
        }
    }

    /// Abre `path` SEM seguir ponto de reparse (`FILE_FLAG_OPEN_REPARSE_POINT`): se o nome for
    /// uma junção/link, o handle é o da própria junção, não o do alvo.
    fn open_no_follow(path: &Path, access: u32) -> std::io::Result<Handle> {
        let w = wide(path.as_os_str());
        // SAFETY: caminho terminado em NUL; sem atributos de segurança nem modelo.
        let h = unsafe {
            win32::CreateFileW(
                w.as_ptr(),
                access,
                win32::FILE_SHARE_ALL,
                std::ptr::null_mut(),
                win32::OPEN_EXISTING,
                win32::FILE_FLAG_BACKUP_SEMANTICS | win32::FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if h == win32::INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Handle(h))
    }

    /// Atributos lidos DO HANDLE (não do nome), para saber o que de fato foi aberto.
    fn handle_attributes(h: &Handle) -> std::io::Result<u32> {
        // SAFETY: `h` é válido; a estrutura é só de saída.
        let mut info: win32::BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { win32::GetFileInformationByHandle(h.0, &mut info) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(info.dwFileAttributes)
    }

    /// O que `harden` encontrou.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Hardened {
        /// Pasta de verdade: dono e DACL aplicados.
        Done,
        /// Junção, link ou não-pasta: NADA foi gravado; quem chama a afasta do caminho.
        NotOurs,
    }

    /// Reaplica o descritor do GuiaLar a uma pasta que já existe: dono Administradores e DACL
    /// protegida (Administradores/SYSTEM total, Usuários leitura, sem herança do pai). É
    /// idempotente, vale mesmo com a pasta criada pelo Explorer (ACL herdada de `ProgramData`)
    /// e com handles abertos por outra conta; assume a posse com `SeTakeOwnership/SeRestore`.
    ///
    /// Abre a pasta UMA vez, sem seguir ponto de reparse, confere pelo próprio handle que é uma
    /// pasta de verdade e aplica o descritor nesse handle (`SetSecurityInfo`). Nunca repete a
    /// operação pelo nome: uma conta comum não consegue trocar o nome por uma junção entre a
    /// checagem e a escrita da ACL (a DACL cairia no alvo da junção).
    pub fn harden(dir: &Path) -> Result<Hardened> {
        #[cfg(test)]
        let sandboxed = crate::testenv::sandboxed();
        #[cfg(not(test))]
        let sandboxed = false;
        if !sandboxed {
            enable_privilege("SeTakeOwnershipPrivilege");
            enable_privilege("SeRestorePrivilege");
            enable_privilege("SeBackupPrivilege");
        }
        let access = if sandboxed {
            win32::READ_CONTROL
        } else {
            win32::READ_CONTROL | win32::WRITE_DAC | win32::WRITE_OWNER
        };
        let h = open_no_follow(dir, access).with_context(|| {
            format!("não foi possível abrir {} para fechar a ACL", dir.display())
        })?;
        let attrs = handle_attributes(&h)
            .with_context(|| format!("não foi possível ler os atributos de {}", dir.display()))?;
        if attrs & win32::FILE_ATTRIBUTE_REPARSE_POINT != 0
            || attrs & win32::FILE_ATTRIBUTE_DIRECTORY == 0
        {
            return Ok(Hardened::NotOurs);
        }
        if sandboxed {
            return Ok(Hardened::Done);
        }
        let sd = security_descriptor()?;
        let mut owner: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut dacl: *mut std::ffi::c_void = std::ptr::null_mut();
        let (mut present, mut defaulted) = (0i32, 0i32);
        // SAFETY: `sd` é um descritor válido; ponteiros de saída válidos e internos a `sd`.
        let ok = unsafe {
            win32::GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted) != 0
                && win32::GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted)
                    != 0
        };
        let r = if ok && present != 0 && !owner.is_null() {
            // SAFETY: handle válido; dono e DACL pertencem a `sd` (vivo até o free).
            unsafe {
                win32::SetSecurityInfo(
                    h.0,
                    win32::SE_FILE_OBJECT,
                    win32::OWNER_SECURITY_INFORMATION
                        | win32::DACL_SECURITY_INFORMATION
                        | win32::PROTECTED_DACL_SECURITY_INFORMATION,
                    owner,
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null_mut(),
                )
            }
        } else {
            u32::MAX
        };
        // SAFETY: `sd` veio da conversão.
        unsafe { win32::LocalFree(sd) };
        if r != 0 {
            bail!(
                "não foi possível aplicar a ACL protegida em {} (erro {r})",
                dir.display()
            );
        }
        Ok(Hardened::Done)
    }

    /// Desvincula o nome na hora (`FileDispositionInfoEx`, semântica POSIX): vale mesmo que
    /// outro processo mantenha o arquivo aberto COM `FILE_SHARE_DELETE`, e o nome fica livre
    /// para o `create_new` seguinte. Um handle SEM `FILE_SHARE_DELETE` (por exemplo, abrir com
    /// `FileShare.None`) impede até a abertura para apagar: o erro sobe (ver `unlink_now`).
    pub fn unlink_now(path: &Path) -> std::io::Result<()> {
        let h = open_no_follow(path, win32::DELETE)?;
        let flags: u32 = win32::FILE_DISPOSITION_UNLINK_NOW;
        // SAFETY: `h` válido; `flags` vive durante a chamada e tem 4 bytes.
        let ok = unsafe {
            win32::SetFileInformationByHandle(
                h.0,
                win32::FILE_DISPOSITION_INFO_EX,
                (&raw const flags).cast(),
                4,
            )
        };
        let err = std::io::Error::last_os_error();
        drop(h);
        if ok != 0 {
            return Ok(());
        }
        // Windows sem a classe «Ex» (anterior ao 10 1709): apagar do jeito comum
        if matches!(err.raw_os_error(), Some(1 | 87)) {
            return std::fs::remove_file(path);
        }
        Err(err)
    }

    /// Apaga o que há na pasta e não é de Administradores/SYSTEM (uma conta comum pode ter
    /// plantado `op.lock`, um `.tmp` previsível ou uma pasta). Não segue junção: remove a junção.
    /// Um nome alheio que não sai (arquivo preso por outro processo): se for `op.lock` ou
    /// `state.json`, erro claro, porque o apply depende deles; qualquer outro, só aviso, já que
    /// a pasta está fechada e esse arquivo nunca é lido.
    fn remove_foreign_entries(dir: &Path) -> Result<()> {
        for e in std::fs::read_dir(dir)
            .with_context(|| format!("não foi possível listar {}", dir.display()))?
            .flatten()
        {
            let p = e.path();
            if is_trusted(&p) {
                continue;
            }
            let Ok(m) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            let reparse = m.file_type().is_symlink()
                || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
            let r = if m.is_dir() && !reparse {
                std::fs::remove_dir_all(&p)
            } else if m.is_dir() {
                std::fs::remove_dir(&p)
            } else {
                unlink_now(&p)
            };
            match r {
                Ok(()) => eprintln!("⚠️ Removido {} (não era do sistema).", p.display()),
                Err(err) if is_core_name(&e.file_name().to_string_lossy()) => bail!(
                    "{} é de outro dono e está aberto por outro programa, que não permite apagá-lo ({err}). Feche esse programa (ou saia da conta que o abriu) e tente de novo.",
                    p.display()
                ),
                Err(err) => eprintln!(
                    "⚠️ Não consegui remover {} ({err}); a pasta está fechada e esse arquivo não é usado.",
                    p.display()
                ),
            }
        }
        Ok(())
    }

    /// Tira o nome do caminho (junção, link ou não-pasta): renomeia com sufixo aleatório.
    fn afastar(dir: &Path) -> std::io::Result<PathBuf> {
        let aside = dir.with_file_name(format!(
            "GuiaLar.recusada-{}",
            random_hex().map_err(std::io::Error::other)?
        ));
        std::fs::rename(dir, &aside)?;
        Ok(aside)
    }

    /// Prepara a pasta de estado:
    /// - não existe: cria já protegida;
    /// - ponto de reparse (junção/link): sai do caminho (renomeada, sem seguir) com sufixo
    ///   aleatório e a certa é criada;
    /// - pasta de verdade, nossa ou de outro dono: assume (dono Administradores + DACL
    ///   protegida, pelo handle) e apaga o que for de dono alheio. Nunca renomeia pasta real,
    ///   que uma conta comum poderia travar com um handle aberto. Se na hora de fechar o nome
    ///   virou junção, nada é gravado e ela é afastada como as outras.
    pub fn ensure_dir() -> Result<PathBuf> {
        let dir = state_dir()?;
        for tentativa in 0..3 {
            let afastar_agora = match std::fs::symlink_metadata(&dir) {
                Ok(m) => {
                    let ja_e_link = m.file_type().is_symlink()
                        || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                        || !m.is_dir();
                    // `harden` só roda em pasta de verdade e devolve `NotOurs` se, ao abrir,
                    // o nome já era outra coisa (troca no meio): nada é gravado nesse caso
                    if ja_e_link || harden(&dir)? == Hardened::NotOurs {
                        true
                    } else {
                        remove_foreign_entries(&dir)?;
                        if !is_trusted(&dir) {
                            bail!("{} continua sem dono do sistema", dir.display());
                        }
                        return Ok(dir);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    match create_protected(&dir) {
                        Ok(()) => {
                            if !is_trusted(&dir) {
                                bail!("{} nasceu sem dono do sistema", dir.display());
                            }
                            return Ok(dir);
                        }
                        // alguém criou no meio: a volta seguinte a trata como pasta existente
                        Err(_) if tentativa < 2 => continue,
                        Err(e) => return Err(e),
                    }
                }
                Err(e) => return Err(e).context("não foi possível checar a pasta de estado"),
            };
            if afastar_agora {
                match afastar(&dir) {
                    Ok(aside) => eprintln!(
                        "⚠️ {} era um link; renomeada para {} e recriada.",
                        dir.display(),
                        aside.display()
                    ),
                    // outro nome ao acaso na próxima volta; esgotadas, o erro sobe
                    Err(_) if tentativa < 2 => continue,
                    Err(e) => {
                        return Err(e)
                            .with_context(|| format!("não foi possível afastar {}", dir.display()))
                    }
                }
            }
        }
        bail!("não foi possível preparar {}", dir.display())
    }

    pub fn write_atomic(dir: &Path, name: &str, data: &[u8]) -> Result<()> {
        // nome aleatório (128 bits) e `create_new`: uma conta comum não adivinha nem pré-cria
        let tmp = dir.join(format!(
            ".{name}.{}.{}.tmp",
            std::process::id(),
            random_hex()?
        ));
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
    pub fn unlink_now(path: &Path) -> std::io::Result<()> {
        std::fs::remove_file(path)
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

    #[test]
    fn so_op_lock_e_state_json_sao_nomes_de_que_o_apply_depende() {
        assert!(is_core_name("op.lock") && is_core_name("state.json"));
        for outro in ["", "OP.LOCK2", "estado.json", ".state.json.1.ab.tmp", "x"] {
            assert!(!is_core_name(outro), "{outro:?}");
        }
    }

    #[test]
    fn unlink_now_apaga_o_nome_e_diz_quando_nao_existe() {
        let s = Sandbox::new("statedir-unlink");
        let f = s.dir().join("op.lock");
        std::fs::write(&f, "1").unwrap();
        unlink_now(&f).unwrap();
        assert!(!f.exists());
        assert_eq!(
            unlink_now(&f).unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );
    }

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

#[cfg(all(test, windows))]
mod tests_win {
    use super::*;
    use crate::testenv::Sandbox;

    #[test]
    fn estado_so_na_pasta_do_sistema_e_le_de_volta() {
        let s = Sandbox::new("statedir-win-ok");
        let dir = state_dir().unwrap();
        assert!(dir.starts_with(s.dir().join("sistema")));
        write_state(b"{\"a\":1}").unwrap();
        assert_eq!(read_state().unwrap().as_deref(), Some(&b"{\"a\":1}"[..]));
        let nomes: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(nomes, [STATE_FILE], "sem .tmp esquecido");
        remove_state().unwrap();
        assert!(read_state().unwrap().is_none());
    }

    #[test]
    fn pasta_preexistente_como_juncao_e_afastada_e_recriada_sem_tocar_o_alvo() {
        let s = Sandbox::new("statedir-win-junction");
        let dir = state_dir().unwrap();
        let fora = s.dir().join("FORA");
        std::fs::create_dir_all(&fora).unwrap();
        std::fs::write(fora.join("state.json"), "{\"forjado\":true}").unwrap();
        crate::safefs::junction(&dir, &fora); // GuiaLar -> FORA (plantada por uma conta comum)
                                              // o estado forjado atrás da junção não é lido
        assert!(read_state().unwrap().is_none());
        write_state(b"{\"ok\":true}").unwrap();
        // a junção saiu do caminho (renomeada, sem seguir) e a pasta certa foi recriada
        let pai = dir.parent().unwrap();
        let afastadas: Vec<_> = std::fs::read_dir(pai)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("GuiaLar.recusada-"))
            .collect();
        assert_eq!(afastadas.len(), 1, "{afastadas:?}");
        assert_eq!(
            std::fs::read_to_string(fora.join("state.json")).unwrap(),
            "{\"forjado\":true}"
        );
        assert_eq!(
            read_state().unwrap().as_deref(),
            Some(&b"{\"ok\":true}"[..])
        );
    }

    /// Toca o `%ProgramData%` de verdade: só como administrador, numa VM, com
    /// `--ignored`. Confere o dono, a DACL protegida e o desvio de uma pasta que não é nossa.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_do_programdata() {
        let dir = state_dir().unwrap();
        assert!(dir.ends_with("GuiaLar"), "{dir:?}");
        let pai = dir.parent().unwrap().to_path_buf();
        let limpar = |dir: &Path, pai: &Path| {
            let _ = std::fs::remove_dir_all(dir);
            for e in std::fs::read_dir(pai).unwrap().flatten() {
                if e.file_name()
                    .to_string_lossy()
                    .starts_with("GuiaLar.recusada-")
                {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        };
        limpar(&dir, &pai);
        let d = ensure_dir().unwrap();
        assert!(is_trusted(&d), "dono tem de ser Administradores ou SYSTEM");
        let icacls = std::process::Command::new("icacls")
            .arg(&d)
            .output()
            .unwrap();
        let acl = String::from_utf8_lossy(&icacls.stdout).into_owned();
        println!("{acl}");
        assert!(acl.contains("Administrators:(OI)(CI)(F)"), "{acl}");
        assert!(acl.contains("SYSTEM:(OI)(CI)(F)"), "{acl}");
        assert!(
            acl.contains("Users:(OI)(CI)(R)"),
            "Usuários só leitura: {acl}"
        );
        assert!(
            !acl.contains("CREATOR OWNER") && !acl.contains("(I)"),
            "sem herança: {acl}"
        );
        // grava, lê e arquivos novos herdam a ACL (Usuários só leem)
        write_state(b"{\"ok\":true}").unwrap();
        assert_eq!(
            read_state().unwrap().as_deref(),
            Some(&b"{\"ok\":true}"[..])
        );
        let f = std::process::Command::new("icacls")
            .arg(d.join(STATE_FILE))
            .output()
            .unwrap();
        let facl = String::from_utf8_lossy(&f.stdout).into_owned();
        println!("{facl}");
        assert!(
            facl.contains("Users:(I)(R)"),
            "o arquivo herda leitura para Usuários: {facl}"
        );
        assert!(
            !facl.contains("Users:(I)(F)") && !facl.contains("(M)") && !facl.contains("(W)"),
            "{facl}"
        );
        // pasta que não é nossa (junção): sai do caminho e a certa é recriada
        remove_state().unwrap();
        std::fs::remove_dir(&d).unwrap();
        let fora = std::env::temp_dir().join("guialar-acl-fora");
        let _ = std::fs::remove_dir_all(&fora);
        std::fs::create_dir_all(&fora).unwrap();
        crate::safefs::junction(&d, &fora);
        let novo = ensure_dir().unwrap();
        assert!(is_trusted(&novo));
        assert!(
            std::fs::read_dir(&fora).unwrap().next().is_none(),
            "nada gravado atrás da junção"
        );
        let afastadas = std::fs::read_dir(&pai)
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("GuiaLar.recusada-")
            })
            .count();
        assert_eq!(afastadas, 1);
        limpar(&dir, &pai);
        let _ = std::fs::remove_dir_all(&fora);
    }

    fn limpar_real(dir: &Path) {
        let pai = dir.parent().unwrap();
        // um processo recém-morto ainda pode segurar a pasta por instantes
        for _ in 0..20 {
            if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        for e in std::fs::read_dir(pai).unwrap().flatten() {
            if e.file_name()
                .to_string_lossy()
                .starts_with("GuiaLar.recusada-")
            {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    fn icacls(p: &Path) -> String {
        let o = std::process::Command::new("icacls")
            .arg(p)
            .output()
            .unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    }

    /// S1: pasta que já existe com a ACL HERDADA de `ProgramData` (criada pelo Explorer, por um
    /// `mkdir` elevado ou pela beta). O `ensure_dir` a fecha: DACL protegida, dono do sistema,
    /// e apaga o `op.lock` plantado com dono alheio.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_pasta_preexistente_com_acl_herdada_e_fechada() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap(); // herda a ACL de ProgramData
        let antes = icacls(&dir);
        println!("ANTES:\n{antes}");
        assert!(
            antes.contains("(I)"),
            "a pasta nasce com ACL herdada: {antes}"
        );
        // uma conta comum plantou a trava (dono Usuários) e um .tmp
        let trava = dir.join("op.lock");
        std::fs::write(&trava, "99999").unwrap();
        imp::test_set_owner(&trava, "O:BU").unwrap();
        assert!(!is_trusted(&trava), "dono Usuários não é confiável");
        let d = ensure_dir().unwrap();
        let depois = icacls(&d);
        println!("DEPOIS:\n{depois}");
        assert!(is_trusted(&d));
        assert!(!depois.contains("(I)"), "sem herança: {depois}");
        assert!(depois.contains("Administrators:(OI)(CI)(F)"), "{depois}");
        assert!(depois.contains("Users:(OI)(CI)(R)"), "{depois}");
        assert!(!trava.exists(), "a trava de dono alheio foi removida");
        // e o apply seguiria: a trava de verdade é criada
        let l = crate::sysutil::OpLock::acquire_in(&d.join("op.lock")).unwrap();
        drop(l);
        limpar_real(&dir);
    }

    /// S2: pasta REAL de outro dono, com um processo da conta comum com ela como diretório
    /// atual (handle aberto). Não é renomeada: a posse é assumida e a ACL reaplicada.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_pasta_alheia_com_handle_aberto_e_assumida_sem_renomear() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap();
        imp::test_set_owner(&dir, "O:BU").unwrap();
        assert!(!is_trusted(&dir), "dono Usuários não é confiável");
        // processo com a pasta como diretório atual (impede o rename)
        let mut filho = std::process::Command::new("ping")
            .args(["-n", "20", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .current_dir(&dir)
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert!(
            std::fs::rename(&dir, dir.with_file_name("GuiaLar.teste-rename")).is_err(),
            "premissa: com o handle aberto o rename falha"
        );
        let r = ensure_dir();
        let _ = filho.kill();
        let _ = filho.wait();
        let d = r.unwrap();
        assert_eq!(d, dir, "a mesma pasta, assumida");
        assert!(is_trusted(&d), "dono voltou a ser Administradores");
        let acl = icacls(&d);
        assert!(
            !acl.contains("(I)") && acl.contains("Users:(OI)(CI)(R)"),
            "{acl}"
        );
        let afastadas = std::fs::read_dir(dir.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("GuiaLar.recusada-")
            })
            .count();
        assert_eq!(afastadas, 0, "nada foi renomeado para o lado");
        limpar_real(&dir);
    }

    /// M1: a ACL só pode cair na pasta de estado, nunca no alvo de uma junção/link posto no
    /// lugar dela. MEDIDO na win11: `SetNamedSecurityInfoW` pelo nome NÃO seguiu junção nem
    /// link de pasta (gravou no próprio ponto de reparse), então o furo descrito no review não
    /// se reproduz nesta versão do Windows; o `harden` por handle (sem seguir reparse) torna
    /// isso uma garantia nossa, e não um comportamento da API. O teste chama o `harden` no
    /// caminho que já é junção ("troca entre a checagem e a escrita") e confere o alvo.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_harden_nao_segue_juncao_e_nao_grava_a_acl_no_alvo() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        let base = std::env::temp_dir();
        let premissa = base.join("guialar-m1-premissa");
        let fora = base.join("guialar-m1-fora");
        for d in [&premissa, &fora] {
            let _ = std::fs::remove_dir_all(d);
            std::fs::create_dir_all(d).unwrap();
        }
        // premissa: a API por nome atravessa a junção (o dono do ALVO muda)
        let dono_antes = imp::owner_sid(&premissa);
        crate::safefs::junction(&dir, &premissa);
        imp::test_set_owner(&dir, "O:BU").unwrap();
        println!(
            "MEDIDA: dono do alvo antes={dono_antes:?} depois={:?}; dono lido pelo nome da junção={:?}",
            imp::owner_sid(&premissa),
            imp::owner_sid(&dir)
        );
        std::fs::remove_dir(&dir).unwrap();
        // mesma medida com um link simbólico de pasta (precisa de privilégio; administrador tem)
        let st = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/D"])
            .arg(&dir)
            .arg(&premissa)
            .output()
            .unwrap();
        if st.status.success() {
            imp::test_set_owner(&dir, "O:BU").unwrap();
            println!(
                "MEDIDA symlink: dono do alvo depois={:?}; dono pelo nome={:?}",
                imp::owner_sid(&premissa),
                imp::owner_sid(&dir)
            );
            std::fs::remove_dir(&dir).unwrap();
        }
        // o conserto: o harden reconhece a junção pelo handle e não grava nada no alvo
        let acl_antes = icacls(&fora);
        let dono_fora = imp::owner_sid(&fora);
        crate::safefs::junction(&dir, &fora);
        assert_eq!(imp::harden(&dir).unwrap(), imp::Hardened::NotOurs);
        assert_eq!(icacls(&fora), acl_antes, "a ACL do alvo não mudou");
        assert_eq!(imp::owner_sid(&fora), dono_fora, "o dono do alvo não mudou");
        // e o ensure_dir completo afasta a junção e recria a pasta certa
        let d = ensure_dir().unwrap();
        assert!(is_trusted(&d));
        assert_eq!(icacls(&fora), acl_antes);
        assert!(std::fs::read_dir(&fora).unwrap().next().is_none());
        limpar_real(&dir);
        for d in [&premissa, &fora] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    /// Cria `dir\nome` de dono Usuários (alheio) e o devolve aberto com o compartilhamento dado.
    fn plantar(dir: &Path, nome: &str, share: u32) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        let f = dir.join(nome);
        std::fs::write(&f, "99999").unwrap();
        imp::test_set_owner(&f, "O:BU").unwrap();
        assert!(!is_trusted(&f));
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(share)
            .open(&f)
            .unwrap()
    }

    /// M2 (nome qualquer): arquivo alheio preso SEM compartilhamento não impede a ativação.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_arquivo_alheio_preso_de_outro_nome_so_avisa() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap();
        let preso = plantar(&dir, "qualquer.txt", 0); // FileShare.None
        let d = ensure_dir().expect("a pasta fecha mesmo com o arquivo preso");
        assert!(
            d.join("qualquer.txt").exists(),
            "não deu para apagar: segue"
        );
        assert!(!icacls(&d).contains("(I)"), "a ACL protegida valeu");
        // o apply segue: a trava e o manifesto saem normalmente
        drop(crate::sysutil::OpLock::acquire_in(&d.join("op.lock")).unwrap());
        write_state(b"{}").unwrap();
        drop(preso);
        limpar_real(&dir);
    }

    /// M2 (`op.lock`/`state.json`): se o outro processo permite apagar (`FILE_SHARE_DELETE`), o
    /// nome é desvinculado na hora e o `create_new` seguinte funciona, mesmo com o handle velho.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_trava_e_manifesto_alheios_abertos_com_share_delete_sao_desvinculados() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap();
        let velha_trava = plantar(&dir, "op.lock", 7);
        let velho_estado = plantar(&dir, STATE_FILE, 7);
        let d = ensure_dir().unwrap();
        assert!(!d.join("op.lock").exists() && !d.join(STATE_FILE).exists());
        let l = crate::sysutil::OpLock::acquire_in(&d.join("op.lock")).unwrap();
        write_state(b"{\"novo\":true}").unwrap();
        assert_eq!(
            read_state().unwrap().as_deref(),
            Some(&b"{\"novo\":true}"[..])
        );
        drop((l, velha_trava, velho_estado));
        limpar_real(&dir);
    }

    /// M2, o limite medido: `op.lock` alheio aberto SEM `FILE_SHARE_DELETE` não pode ser
    /// apagado nem renomeado por ninguém enquanto o outro processo o segurar. O apply para
    /// com um erro claro (antes de mexer no DNS) e volta a funcionar quando o handle fecha.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_trava_alheia_presa_sem_share_delete_da_erro_claro_e_se_recupera() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap();
        let preso = plantar(&dir, "op.lock", 3); // READ | WRITE, sem DELETE
        let e = format!("{:#}", ensure_dir().unwrap_err());
        assert!(e.contains("aberto por outro programa"), "{e}");
        assert!(!icacls(&dir).contains("(I)"), "a pasta já estava fechada");
        drop(preso);
        let d = ensure_dir().expect("sem o handle, a ativação segue");
        assert!(!d.join("op.lock").exists());
        limpar_real(&dir);
    }
}

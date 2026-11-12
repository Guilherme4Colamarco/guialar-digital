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

/// Uma ACE da DACL de um arquivo, já reduzida ao que importa para a confiança.
#[cfg(any(windows, test))]
#[derive(Debug, Clone)]
struct AceInfo {
    /// `true` para «permitir»; ACEs de negar são ignoradas (só restringem).
    allow: bool,
    mask: u32,
    sid: String,
}

/// Direitos que deixam alterar o arquivo, apagá-lo ou trocar sua ACL/dono: `FILE_WRITE_DATA`,
/// `FILE_APPEND_DATA`, `FILE_WRITE_EA`, `FILE_DELETE_CHILD`, `FILE_WRITE_ATTRIBUTES`, `DELETE`,
/// `WRITE_DAC`, `WRITE_OWNER`, `GENERIC_WRITE` e `GENERIC_ALL`.
#[cfg(any(windows, test))]
const WRITE_LIKE_MASK: u32 =
    0x2 | 0x4 | 0x10 | 0x40 | 0x100 | 0x1_0000 | 0x4_0000 | 0x8_0000 | 0x4000_0000 | 0x1000_0000;

/// A DACL de um ARQUIVO do estado é segura? Protegida (não herda da pasta, que uma conta comum
/// pode ter reaberto, N1) e sem direito de escrita para quem não seja Administradores/SYSTEM.
/// `aces = None` é uma DACL nula (todos com tudo).
#[cfg(any(windows, test))]
fn file_dacl_is_safe(protected: bool, aces: Option<&[AceInfo]>) -> bool {
    let Some(aces) = aces else { return false };
    protected
        && aces.iter().all(|a| {
            !a.allow
                || a.mask & WRITE_LIKE_MASK == 0
                || a.sid == "S-1-5-32-544"
                || a.sid == "S-1-5-18"
        })
}

/// Cria um arquivo NOVO (`CREATE_NEW`) já com dono Administradores e DACL protegida explícita
/// (Administradores e SYSTEM controle total, Usuários só leitura), sem herdar da pasta.
#[cfg(windows)]
pub fn create_new_protected(path: &Path) -> std::io::Result<std::fs::File> {
    imp::create_new_protected(path)
}

/// Grava o manifesto (temp + `rename`) na pasta do sistema.
pub fn write_state(data: &[u8]) -> Result<()> {
    let dir = ensure_dir()?;
    imp::write_atomic(&dir, STATE_FILE, data)
}

/// Marca (como contexto do `anyhow`) uma falha de E/S ao LER o registro, por exemplo o arquivo
/// aberto por outro programa: diferente de um registro inválido ou não confiável. Quem decide o
/// que fazer com o erro (`revert`, status) usa `is_state_io_error` para não tratar a falha de
/// leitura como «registro danificado» e apagá-lo.
#[derive(Debug)]
pub struct StateIo;

impl std::fmt::Display for StateIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("o registro da proteção não pôde ser lido (provavelmente está aberto por outro programa)")
    }
}

/// O erro é uma falha de E/S ao ler o registro (e não conteúdo inválido)?
pub fn is_state_io_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<StateIo>().is_some()
}

/// Resultado de ler um arquivo do estado por um único handle.
#[derive(Debug)]
pub enum ReadOutcome {
    Missing,
    /// Link/reparse, não é arquivo comum, ou dono/DACL que não são os do sistema.
    Untrusted,
    Data(Vec<u8>),
}

/// Teto do manifesto lido (o legítimo tem poucos KB).
const MAX_STATE_BYTES: u64 = 1 << 20;

/// Lê o manifesto; `None` se não existe ou se pasta/arquivo não são de dono confiável (nesse
/// caso avisa e ignora). O arquivo é aberto UMA vez, conferido pelo próprio handle e lido
/// dele: conferir por nome e abrir por nome depois deixaria uma conta comum, que tenha
/// reaberto a pasta (resíduo do N1), trocar o arquivo entre a checagem e a leitura (N2).
pub fn read_state() -> Result<Option<Vec<u8>>> {
    let dir = state_dir()?;
    if std::fs::symlink_metadata(&dir).is_err() {
        return Ok(None);
    }
    let file = dir.join(STATE_FILE);
    let ignorar = || {
        eprintln!(
            "⚠️ Ignorando {}: o dono não é o do sistema (o registro da proteção não foi usado).",
            file.display()
        );
        Ok(None)
    };
    if !is_trusted(&dir) {
        return ignorar();
    }
    match imp::read_trusted_file(&file)
        .map_err(|e| anyhow::Error::new(e).context(StateIo))
        .with_context(|| format!("Falha ao ler manifesto em {file:?}"))?
    {
        ReadOutcome::Missing => Ok(None),
        ReadOutcome::Untrusted => ignorar(),
        ReadOutcome::Data(b) => Ok(Some(b)),
    }
}

/// Tenta de novo por até ~1 s quando o arquivo está momentaneamente aberto por um leitor
/// (no Windows o leitor do estado não compartilha `DELETE`, então um `rename` ou apagar que
/// caia no meio de uma leitura recebe acesso negado/violação de compartilhamento).
fn retry_busy<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tentativa = 0;
    loop {
        match f() {
            Err(e)
                if cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32)) && tentativa < 50 =>
            {
                tentativa += 1;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            r => return r,
        }
    }
}

/// Apaga o manifesto do sistema (só o nome do arquivo).
pub fn remove_state() -> Result<()> {
    let file = state_file()?;
    match std::fs::symlink_metadata(&file) {
        Ok(_) => retry_busy(|| std::fs::remove_file(&file))
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

    /// Abre sem seguir link e confere dono/modo pelo próprio descritor; lê dele. (No Linux só o
    /// root grava na pasta, mas o mesmo desenho vale: nada é aberto duas vezes por nome.)
    pub fn read_trusted_file(path: &Path) -> std::io::Result<ReadOutcome> {
        use std::io::Read;
        let f = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(crate::safefs::O_NOFOLLOW)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ReadOutcome::Missing),
            // ELOOP: o nome é um link
            Err(e) if e.raw_os_error() == Some(40) => return Ok(ReadOutcome::Untrusted),
            Err(e) => return Err(e),
        };
        let m = f.metadata()?;
        if !m.is_file() || m.uid() != trusted_uid() || m.mode() & 0o022 != 0 {
            return Ok(ReadOutcome::Untrusted);
        }
        let mut buf = Vec::new();
        f.take(MAX_STATE_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_STATE_BYTES {
            return Ok(ReadOutcome::Untrusted);
        }
        Ok(ReadOutcome::Data(buf))
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
        if !owner_sid(path).is_some_and(|s| TRUSTED_SIDS.contains(&s.as_str())) {
            return false;
        }
        // Arquivo: a DACL também conta (N1). Uma conta comum que tenha mantido um handle com
        // WRITE_DAC na pasta antes do primeiro fechamento pode reabri-la; o dono do arquivo
        // seria Administradores mesmo assim, mas gravável por ela.
        if m.is_file() {
            return read_dacl(path)
                .is_some_and(|(prot, aces)| file_dacl_is_safe(prot, aces.as_deref()));
        }
        true
    }

    /// DACL do objeto: (protegida?, ACEs). `None` se não deu para ler; ACEs `None` = DACL nula.
    fn read_dacl(path: &Path) -> Option<(bool, Option<Vec<AceInfo>>)> {
        let w = wide(path.as_os_str());
        let mut dacl: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: nome terminado em NUL e ponteiros de saída válidos; `sd` é liberado abaixo.
        let r = unsafe {
            win32::GetNamedSecurityInfoW(
                w.as_ptr(),
                win32::SE_FILE_OBJECT,
                win32::DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        if r != 0 || sd.is_null() {
            return None;
        }
        let out = decode_dacl(sd, dacl);
        // SAFETY: `sd` veio do GetNamedSecurityInfoW.
        unsafe { win32::LocalFree(sd) };
        out
    }

    /// Lê a DACL de `sd`/`dacl` (ponteiros vivos do chamador): (protegida?, ACEs).
    fn decode_dacl(
        sd: *mut std::ffi::c_void,
        dacl: *mut std::ffi::c_void,
    ) -> Option<(bool, Option<Vec<AceInfo>>)> {
        let mut control: u16 = 0;
        let mut rev: u32 = 0;
        // SAFETY: `sd` válido até o LocalFree; saídas válidas.
        let ok = unsafe { win32::GetSecurityDescriptorControl(sd, &mut control, &mut rev) } != 0;
        ok.then(|| {
            let protected = control & win32::SE_DACL_PROTECTED != 0;
            if dacl.is_null() {
                return (protected, None);
            }
            let mut info = win32::ACL_SIZE_INFORMATION {
                AceCount: 0,
                AclBytesInUse: 0,
                AclBytesFree: 0,
            };
            // SAFETY: `dacl` aponta para dentro de `sd`; `info` tem o tamanho da classe.
            let got = unsafe {
                win32::GetAclInformation(
                    dacl,
                    (&raw mut info).cast(),
                    std::mem::size_of::<win32::ACL_SIZE_INFORMATION>() as u32,
                    win32::ACL_SIZE_INFORMATION_CLASS,
                )
            } != 0;
            let mut aces = Vec::new();
            if !got {
                // não leu a lista: trata como DACL desconhecida (nunca confiável)
                return (protected, None);
            }
            for i in 0..info.AceCount {
                let mut ace: *mut std::ffi::c_void = std::ptr::null_mut();
                // SAFETY: índice dentro de `AceCount`; `ace` aponta para dentro da DACL.
                if unsafe { win32::GetAce(dacl, i, &mut ace) } == 0 || ace.is_null() {
                    return (protected, None);
                }
                // ACE_HEADER {tipo u8, flags u8, tamanho u16}; as ACEs de permitir/negar
                // «simples» (tipos 0 e 1) seguem com Mask u32 e o SID a partir do byte 8.
                // SAFETY: cabeçalho de 4 bytes e, para os tipos 0/1, máscara e SID válidos.
                let tipo = unsafe { *(ace as *const u8) };
                if tipo > 1 {
                    // qualquer outro tipo (objeto, callback, auditoria...): desconhecido
                    aces.push(AceInfo {
                        allow: true,
                        mask: u32::MAX,
                        sid: "?".into(),
                    });
                    continue;
                }
                // SAFETY: ver acima.
                let mask = unsafe { *((ace as *const u8).add(4) as *const u32) };
                let mut s: *mut u16 = std::ptr::null_mut();
                let sid = unsafe { (ace as *mut u8).add(8) };
                // SAFETY: o SID está dentro da ACE; a string é liberada abaixo.
                let conv = unsafe { win32::ConvertSidToStringSidW(sid.cast(), &mut s) };
                let sid = if conv != 0 && !s.is_null() {
                    let t = from_wide_ptr(s);
                    // SAFETY: veio de ConvertSidToStringSidW.
                    unsafe { win32::LocalFree(s.cast()) };
                    t
                } else {
                    "?".into()
                };
                aces.push(AceInfo {
                    allow: tipo == 0,
                    mask,
                    sid,
                });
            }
            (protected, Some(aces))
        })
    }

    /// Dono (SID em texto) e DACL de um HANDLE (`GetSecurityInfo`), não de um nome.
    fn security_of_handle(
        h: win32::HANDLE,
    ) -> Option<(Option<String>, bool, Option<Vec<AceInfo>>)> {
        let mut owner: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut dacl: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut sd: *mut std::ffi::c_void = std::ptr::null_mut();
        // SAFETY: handle válido e ponteiros de saída válidos; `sd` é liberado abaixo.
        let r = unsafe {
            win32::GetSecurityInfo(
                h,
                win32::SE_FILE_OBJECT,
                win32::OWNER_SECURITY_INFORMATION | win32::DACL_SECURITY_INFORMATION,
                &mut owner,
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        if r != 0 || sd.is_null() {
            return None;
        }
        let mut s: *mut u16 = std::ptr::null_mut();
        // SAFETY: `owner` aponta para dentro de `sd` (vivo até o LocalFree abaixo).
        let ok = !owner.is_null() && unsafe { win32::ConvertSidToStringSidW(owner, &mut s) } != 0;
        let owner_text = (ok && !s.is_null()).then(|| from_wide_ptr(s));
        if !s.is_null() {
            // SAFETY: veio de ConvertSidToStringSidW.
            unsafe { win32::LocalFree(s.cast()) };
        }
        let out = decode_dacl(sd, dacl).map(|(prot, aces)| (owner_text, prot, aces));
        // SAFETY: `sd` veio do GetSecurityInfo.
        unsafe { win32::LocalFree(sd) };
        out
    }

    /// Abre `path` UMA vez (leitura, sem `FILE_SHARE_DELETE` nem de escrita, sem seguir ponto
    /// de reparse), confere pelo handle que é um arquivo comum de dono Administradores/SYSTEM
    /// com DACL segura e lê desse mesmo handle. Sem `FILE_SHARE_DELETE`, ninguém renomeia nem
    /// apaga o arquivo enquanto ele está aberto, então o que foi conferido é o que é lido.
    pub fn read_trusted_file(path: &Path) -> std::io::Result<ReadOutcome> {
        use std::io::Read;
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        let w = wide(path.as_os_str());
        // Outro leitor/escritor pode estar com o arquivo aberto por instantes (o nosso `rename`,
        // por exemplo): repete a abertura por até ~1 s em acesso negado/violação de
        // compartilhamento, como o escritor faz.
        let mut tentativa = 0;
        let h = loop {
            // SAFETY: caminho terminado em NUL; sem atributos de segurança nem modelo.
            let h = unsafe {
                win32::CreateFileW(
                    w.as_ptr(),
                    win32::GENERIC_READ | win32::READ_CONTROL,
                    win32::FILE_SHARE_READ,
                    std::ptr::null_mut(),
                    win32::OPEN_EXISTING,
                    win32::FILE_ATTRIBUTE_NORMAL | win32::FILE_FLAG_OPEN_REPARSE_POINT,
                    std::ptr::null_mut(),
                )
            };
            if h != win32::INVALID_HANDLE_VALUE {
                break h;
            }
            let e = std::io::Error::last_os_error();
            match e.raw_os_error() {
                Some(2 | 3) => return Ok(ReadOutcome::Missing),
                // pasta no lugar do arquivo (sem BACKUP_SEMANTICS não abre): não é nosso
                Some(5) if path.is_dir() => return Ok(ReadOutcome::Untrusted),
                Some(5 | 32) if tentativa < 50 => {
                    tentativa += 1;
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                _ => return Err(e),
            }
        };
        // SAFETY: handle de arquivo válido, de posse única; o `File` o fecha.
        let f = unsafe { std::fs::File::from_raw_handle(h) };
        let attrs = raw_handle_attributes(f.as_raw_handle())?;
        if attrs & win32::FILE_ATTRIBUTE_REPARSE_POINT != 0
            || attrs & win32::FILE_ATTRIBUTE_DIRECTORY != 0
        {
            return Ok(ReadOutcome::Untrusted);
        }
        #[cfg(test)]
        let sandboxed = crate::testenv::sandboxed();
        #[cfg(not(test))]
        let sandboxed = false;
        if !sandboxed {
            let seguro = security_of_handle(f.as_raw_handle()).is_some_and(|(dono, prot, aces)| {
                dono.is_some_and(|d| TRUSTED_SIDS.contains(&d.as_str()))
                    && file_dacl_is_safe(prot, aces.as_deref())
            });
            if !seguro {
                return Ok(ReadOutcome::Untrusted);
            }
        }
        let mut buf = Vec::new();
        f.take(MAX_STATE_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_STATE_BYTES {
            return Ok(ReadOutcome::Untrusted);
        }
        Ok(ReadOutcome::Data(buf))
    }

    /// `CREATE_NEW` com dono Administradores e DACL protegida explícita (ver o nível acima).
    pub fn create_new_protected(path: &Path) -> std::io::Result<std::fs::File> {
        use std::os::windows::io::FromRawHandle;
        #[cfg(test)]
        if crate::testenv::sandboxed() {
            // nos testes unitários a conta de teste não é administradora: arquivo comum
            return std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path);
        }
        // dono Administradores, grupo SYSTEM, DACL protegida; Usuários só leem
        let sd = security_descriptor_from("O:BAG:SYD:P(A;;FA;;;BA)(A;;FA;;;SY)(A;;FR;;;BU)")
            .map_err(std::io::Error::other)?;
        let sa = win32::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<win32::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        let w = wide(path.as_os_str());
        // SAFETY: caminho terminado em NUL e atributos válidos durante a chamada. Compartilha
        // leitura/escrita/apagar como o `std` (o usuário lê o PID da trava).
        let h = unsafe {
            win32::CreateFileW(
                w.as_ptr(),
                win32::GENERIC_WRITE,
                win32::FILE_SHARE_ALL,
                (&raw const sa).cast_mut().cast(),
                win32::CREATE_NEW,
                win32::FILE_ATTRIBUTE_NORMAL | win32::FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        let err = std::io::Error::last_os_error();
        // SAFETY: `sd` veio da conversão.
        unsafe { win32::LocalFree(sd) };
        if h == win32::INVALID_HANDLE_VALUE {
            return Err(err);
        }
        // SAFETY: handle de arquivo válido, de posse única; o `File` o fecha.
        Ok(unsafe { std::fs::File::from_raw_handle(h) })
    }

    /// Só para testes de ACL real: o que um atacante com um handle `WRITE_DAC` faz — reabre a
    /// DACL da pasta dando controle total a Usuários. Devolve o código do `SetSecurityInfo`.
    #[cfg(test)]
    pub fn test_grant_users_full(h: &Handle) -> u32 {
        let sd = security_descriptor_from("D:(A;OICI;FA;;;BU)").unwrap();
        let mut dacl: *mut std::ffi::c_void = std::ptr::null_mut();
        let (mut present, mut defaulted) = (0i32, 0i32);
        // SAFETY: `sd` válido; saídas internas a `sd`.
        unsafe { win32::GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) };
        // SAFETY: handle válido; `dacl` pertence a `sd`.
        let r = unsafe {
            win32::SetSecurityInfo(
                h.0,
                win32::SE_FILE_OBJECT,
                win32::DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        };
        // SAFETY: veio da conversão.
        unsafe { win32::LocalFree(sd) };
        r
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
    pub struct Handle(win32::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: o handle veio de `CreateFileW` e é fechado uma vez.
            unsafe { win32::CloseHandle(self.0) };
        }
    }

    /// Abre `path` SEM seguir ponto de reparse (`FILE_FLAG_OPEN_REPARSE_POINT`): se o nome for
    /// uma junção/link, o handle é o da própria junção, não o do alvo.
    pub fn open_no_follow(path: &Path, access: u32) -> std::io::Result<Handle> {
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
        raw_handle_attributes(h.0)
    }

    fn raw_handle_attributes(h: win32::HANDLE) -> std::io::Result<u32> {
        // SAFETY: `h` é válido; a estrutura é só de saída.
        let mut info: win32::BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { win32::GetFileInformationByHandle(h, &mut info) } == 0 {
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
            // DACL protegida explícita (não herda da pasta, que pode ter sido reaberta, N1)
            let mut f = create_new_protected(&tmp)
                .with_context(|| format!("Falha ao criar arquivo temporário {tmp:?}"))?;
            f.write_all(data)?;
            f.flush()?;
            drop(f);
            retry_busy(|| std::fs::rename(&tmp, &target)).map_err(|e| {
                if matches!(e.raw_os_error(), Some(5 | 32)) {
                    // esgotadas as tentativas: alguém mantém o registro aberto
                    anyhow::anyhow!(
                        "O registro da proteção está aberto por outro programa e não pôde ser atualizado ({e}). Feche esse programa e tente de novo."
                    )
                } else {
                    anyhow::Error::new(e)
                        .context(format!("Falha ao renomear {tmp:?} para {target:?}"))
                }
            })
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
    pub fn read_trusted_file(_: &Path) -> std::io::Result<ReadOutcome> {
        Err(std::io::Error::other("plataforma sem suporte"))
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

    fn ace(allow: bool, mask: u32, sid: &str) -> AceInfo {
        AceInfo {
            allow,
            mask,
            sid: sid.into(),
        }
    }

    #[test]
    fn dacl_de_arquivo_so_e_segura_protegida_e_sem_escrita_para_outros() {
        const FA: u32 = 0x1F_01FF;
        const FR: u32 = 0x12_0089;
        let (ba, sy, bu) = ("S-1-5-32-544", "S-1-5-18", "S-1-5-32-545");
        let boa = [ace(true, FA, ba), ace(true, FA, sy), ace(true, FR, bu)];
        assert!(file_dacl_is_safe(true, Some(&boa)));
        // não protegida: herdaria da pasta (N1)
        assert!(!file_dacl_is_safe(false, Some(&boa)));
        // DACL nula = todos com tudo
        assert!(!file_dacl_is_safe(true, None));
        // Usuários com qualquer direito de escrita, apagar ou mexer na ACL/dono
        for perigo in [
            FA,
            0x2,
            0x4,
            0x10,
            0x40,
            0x100,
            0x1_0000,
            0x4_0000,
            0x8_0000,
            0x4000_0000,
            0x1000_0000,
        ] {
            let ruim = [ace(true, FA, ba), ace(true, perigo, bu)];
            assert!(!file_dacl_is_safe(true, Some(&ruim)), "{perigo:#x}");
        }
        // «Todos» e o próprio usuário também valem
        for sid in ["S-1-1-0", "S-1-5-21-1-2-3-1001", "S-1-5-11", "?"] {
            assert!(
                !file_dacl_is_safe(true, Some(&[ace(true, 0x2, sid)])),
                "{sid}"
            );
        }
        // negar não concede nada
        let com_negar = [ace(true, FA, ba), ace(false, FA, bu), ace(true, FR, bu)];
        assert!(file_dacl_is_safe(true, Some(&com_negar)));
    }

    #[test]
    fn leitura_por_um_descritor_ausente_link_e_arquivo_grande() {
        let s = Sandbox::new("statedir-leitura");
        write_state(b"{}").unwrap();
        let dir = state_dir().unwrap();
        let f = dir.join(STATE_FILE);
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Data(b) if b == b"{}"
        ));
        // ausente
        remove_state().unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Missing
        ));
        // link no lugar do arquivo: não é seguido
        let alvo = s.dir().join("alvo.json");
        std::fs::write(&alvo, "{\"forjado\":true}").unwrap();
        symlink(&alvo, &f).unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Untrusted
        ));
        assert!(read_state().unwrap().is_none());
        std::fs::remove_file(&f).unwrap();
        // grande demais: o legítimo tem poucos KB
        let grande = vec![b'x'; (MAX_STATE_BYTES + 1) as usize];
        std::fs::write(&f, &grande).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Untrusted
        ));
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

    /// B2: a abertura do leitor repete em violação de compartilhamento; só desiste (com erro de
    /// E/S marcado, não «registro inválido») se o arquivo continua preso.
    #[test]
    fn leitura_repete_a_abertura_e_so_desiste_com_erro_de_es_marcado() {
        use std::os::windows::fs::OpenOptionsExt;
        let _s = Sandbox::new("statedir-win-leitura-preso");
        write_state(b"{\"a\":1}").unwrap();
        let f = state_dir().unwrap().join(STATE_FILE);
        // preso só por um instante: a leitura espera e passa
        let preso = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&f)
            .unwrap();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(preso);
        });
        assert_eq!(read_state().unwrap().as_deref(), Some(&b"{\"a\":1}"[..]));
        t.join().unwrap();
        // preso por mais tempo que as tentativas: erro de E/S marcado
        let preso = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&f)
            .unwrap();
        let e = read_state().unwrap_err();
        assert!(is_state_io_error(&e), "{e:#}");
        drop(preso);
        assert!(read_state().unwrap().is_some());
    }

    /// B1: escritor com o registro preso por mais tempo que as tentativas: mensagem clara.
    #[test]
    fn escritor_com_registro_preso_diz_que_esta_aberto_por_outro_programa() {
        use std::os::windows::fs::OpenOptionsExt;
        let _s = Sandbox::new("statedir-win-escritor-preso");
        write_state(b"{}").unwrap();
        let f = state_dir().unwrap().join(STATE_FILE);
        let preso = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&f)
            .unwrap();
        let e = format!("{:#}", write_state(b"{\"novo\":1}").unwrap_err());
        assert!(e.contains("aberto por outro programa"), "{e}");
        drop(preso);
        // nenhum temp esquecido e o antigo intacto
        let nomes: Vec<_> = std::fs::read_dir(state_dir().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(nomes, [STATE_FILE]);
        assert_eq!(read_state().unwrap().as_deref(), Some(&b"{}"[..]));
    }

    #[test]
    fn leitura_por_um_handle_ausente_dado_diretorio_e_tamanho() {
        let _s = Sandbox::new("statedir-win-leitura");
        write_state(b"{\"a\":1}").unwrap();
        let f = state_dir().unwrap().join(STATE_FILE);
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Data(b) if b == b"{\"a\":1}"
        ));
        remove_state().unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Missing
        ));
        // uma pasta chamada state.json não é um arquivo nosso
        std::fs::create_dir(&f).unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Untrusted
        ));
        std::fs::remove_dir(&f).unwrap();
        std::fs::write(&f, vec![b'x'; (MAX_STATE_BYTES + 1) as usize]).unwrap();
        assert!(matches!(
            imp::read_trusted_file(&f).unwrap(),
            ReadOutcome::Untrusted
        ));
    }

    /// Enquanto o leitor segura o arquivo (sem `FILE_SHARE_DELETE`), ninguém o renomeia nem
    /// apaga; o escritor espera e passa depois (não pode falhar por causa de uma leitura).
    #[test]
    fn escritor_nao_falha_por_causa_de_leitor_em_curso() {
        let _s = Sandbox::new("statedir-win-leitor");
        write_state(b"{\"n\":0}").unwrap();
        let parar = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let p2 = parar.clone();
        let leitor = std::thread::spawn(move || {
            let mut n = 0u32;
            while !p2.load(std::sync::atomic::Ordering::Relaxed) {
                // se cair numa troca, devolve o erro de abrir (acesso negado); sem erro é ok
                let _ = read_state();
                n += 1;
            }
            n
        });
        for i in 1..=60 {
            write_state(format!("{{\"n\":{i}}}").as_bytes()).unwrap();
        }
        parar.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(leitor.join().unwrap() > 0);
        assert_eq!(read_state().unwrap().as_deref(), Some(&b"{\"n\":60}"[..]));
    }

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
        // N1: o manifesto NÃO herda da pasta; tem DACL própria, protegida, com Usuários só leitura
        assert!(
            facl.contains("Users:(R)") && !facl.contains("(I)"),
            "DACL explícita, sem herança, leitura para Usuários: {facl}"
        );
        assert!(
            !facl.contains("Users:(F)") && !facl.contains("(M)") && !facl.contains("(W)"),
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

    /// Sonda do N1 (medição manual com `atacante.ps1` numa conta sem admin): espera a conta
    /// comum criar a pasta, fecha com `ensure_dir` e grava o manifesto várias vezes enquanto o
    /// atacante reabre a DACL pelo handle antigo. Imprime o que ficou.
    #[test]
    #[ignore = "medição manual do N1 na win11 (precisa do atacante.ps1 rodando)"]
    fn n1_sonda_manifesto_com_atacante_segurando_handle_write_dac() {
        let dir = state_dir().unwrap();
        for _ in 0..100 {
            if dir.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(dir.exists(), "o atacante não criou a pasta");
        std::thread::sleep(std::time::Duration::from_millis(1500)); // o handle já está aberto
        let t0 = std::time::Instant::now();
        let (mut i, mut herdou_escrita) = (0, 0);
        while t0.elapsed() < std::time::Duration::from_secs(14) {
            i += 1;
            match write_state(format!("{{\"n\":{i}}}").as_bytes()) {
                Ok(()) => {}
                Err(e) => println!("write_state #{i} falhou: {e:#}"),
            }
            // o manifesto recém-gravado deu escrita a «Users» por herança?
            let acl = icacls(&dir.join(STATE_FILE));
            if acl.contains("Users:(I)(F)")
                || acl.contains("Users:(I)(M)")
                || acl.contains("Users:(I)(W)")
            {
                herdou_escrita += 1;
            }
        }
        println!(
            "MEDIDA: {i} gravações do manifesto; {herdou_escrita} nasceram com escrita para Users"
        );
        let st = dir.join(STATE_FILE);
        println!("PASTA:\n{}", icacls(&dir));
        println!("STATE.JSON:\n{}", icacls(&st));
        println!(
            "is_trusted(state.json)={} is_trusted(pasta)={} dono(state.json)={:?}",
            is_trusted(&st),
            is_trusted(&dir),
            imp::owner_sid(&st)
        );
    }

    /// N1: uma conta comum que criou a pasta e MANTÉM um handle com `WRITE_DAC` reabre a DACL
    /// (Usuários com controle total) enquanto o `ensure_dir` fecha e o manifesto é gravado.
    /// O manifesto e a trava não herdam da pasta: nascem com DACL protegida, sem escrita para
    /// Usuários, e um arquivo que NÃO esteja assim é ignorado como se fosse de dono alheio.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_n1_handle_write_dac_aberto_antes_nao_forja_manifesto_nem_trava() {
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        std::fs::create_dir(&dir).unwrap(); // criada pela «conta comum»: herda a ACL
        let parar = std::sync::Arc::new(AtomicBool::new(false));
        let reabertas = std::sync::Arc::new(AtomicU32::new(0));
        let (p2, r2) = (parar.clone(), reabertas.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let dir2 = dir.clone();
        let atacante = std::thread::spawn(move || {
            // WRITE_DAC | READ_CONTROL, aberto ANTES do primeiro fechamento da pasta
            let h = imp::open_no_follow(&dir2, 0x4_0000 | 0x2_0000).unwrap();
            tx.send(()).unwrap();
            while !p2.load(Ordering::Relaxed) {
                if imp::test_grant_users_full(&h) == 0 {
                    r2.fetch_add(1, Ordering::Relaxed);
                }
            }
            drop(h);
        });
        rx.recv().unwrap(); // o handle já está aberto
        let st = dir.join(STATE_FILE);
        let lk = dir.join("op.lock");
        let mut ruins = Vec::new();
        for i in 0..150 {
            if let Err(e) = write_state(format!("{{\"n\":{i}}}").as_bytes()) {
                ruins.push(format!("write_state #{i}: {e:#}"));
                continue;
            }
            let a = icacls(&st);
            if a.contains("Users:(I)") || a.contains("Users:(F)") || a.contains("Users:(M)") {
                ruins.push(format!("#{i}: manifesto com escrita para Users:\n{a}"));
            }
            // a trava, criada pelo caminho de produção
            match crate::sysutil::OpLock::acquire_in(&lk) {
                Ok(l) => {
                    let a = icacls(&lk);
                    if a.contains("Users:(I)") || a.contains("Users:(F)") || a.contains("Users:(M)")
                    {
                        ruins.push(format!("#{i}: trava com escrita para Users:\n{a}"));
                    }
                    drop(l);
                }
                Err(e) => ruins.push(format!("acquire_in #{i}: {e:#}")),
            }
        }
        parar.store(true, Ordering::Relaxed);
        atacante.join().unwrap();
        println!(
            "o atacante reabriu a DACL da pasta {} vezes",
            reabertas.load(Ordering::Relaxed)
        );
        assert!(
            reabertas.load(Ordering::Relaxed) > 0,
            "premissa: o handle WRITE_DAC continua válido depois do fechamento"
        );
        assert!(ruins.is_empty(), "{}", ruins.join("\n"));
        limpar_real(&dir);
    }

    /// N1 (b): `state.json` com DACL herdada ou com escrita para Usuários (por exemplo, de uma
    /// versão anterior ou forjado) não é confiável: o `read_state` o ignora, avisando.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_n1_manifesto_com_dacl_herdada_ou_gravavel_nao_e_confiavel() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        let d = ensure_dir().unwrap();
        write_state(b"{\"ok\":true}").unwrap();
        let st = d.join(STATE_FILE);
        assert!(is_trusted(&st), "o manifesto da produção é confiável");
        assert!(read_state().unwrap().is_some());
        // 1) DACL herdada da pasta (não protegida), mesmo com dono Administradores
        let o = std::process::Command::new("icacls")
            .arg(&st)
            .arg("/inheritance:e")
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        assert!(!is_trusted(&st), "ACL não protegida: {}", icacls(&st));
        assert!(read_state().unwrap().is_none(), "ignorado");
        // 2) protegida, mas com escrita para Usuários
        write_state(b"{}").unwrap();
        let o = std::process::Command::new("icacls")
            .arg(&st)
            .args([
                "/inheritance:r",
                "/grant",
                "Users:(M)",
                "Administrators:(F)",
                "SYSTEM:(F)",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        assert!(!is_trusted(&st), "Usuários com M: {}", icacls(&st));
        assert!(read_state().unwrap().is_none());
        // 3) a trava segue a mesma regra
        let lk = d.join("op.lock");
        std::fs::write(&lk, std::process::id().to_string()).unwrap(); // herda da pasta
        assert!(!is_trusted(&lk), "trava com ACL herdada: {}", icacls(&lk));
        std::fs::remove_file(&lk).unwrap();
        limpar_real(&dir);
    }

    /// Sonda do N2 (medição manual com `atacante-n2.ps1` numa conta sem admin): grava o
    /// manifesto legítimo e lê com `read_state` em laço enquanto o atacante alterna, por
    /// renomeação, o `state.json` do admin e um forjado dele. Conta quantas leituras aceitaram
    /// o forjado.
    #[test]
    #[ignore = "medição manual do N2 na win11 (precisa do atacante-n2.ps1 rodando)"]
    fn n2_sonda_read_state_nao_aceita_o_forjado_trocado_no_meio() {
        let dir = state_dir().unwrap();
        for _ in 0..100 {
            if dir.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(dir.exists(), "o atacante não criou a pasta");
        std::thread::sleep(std::time::Duration::from_millis(1500)); // o handle já está aberto
        ensure_dir().unwrap();
        write_state(b"{\"legit\":true}").unwrap();
        let (mut legit, mut forjado, mut nenhum, mut erros) = (0u32, 0u32, 0u32, 0u32);
        let t0 = std::time::Instant::now();
        while t0.elapsed() < std::time::Duration::from_secs(15) {
            match read_state() {
                Ok(Some(b)) if b.windows(7).any(|w| w == b"forjado") => forjado += 1,
                Ok(Some(_)) => legit += 1,
                Ok(None) => nenhum += 1,
                Err(_) => erros += 1,
            }
        }
        println!(
            "MEDIDA N2: {} leituras; legítimo={legit} FORJADO_ACEITO={forjado} ignorado={nenhum} erro={erros}",
            legit + forjado + nenhum + erros
        );
        assert_eq!(forjado, 0, "o forjado foi aceito");
        assert!(legit > 0, "nenhuma leitura legítima");
    }

    /// N2 (determinístico): o arquivo lido tem de ser o conferido. Forjado de dono Usuários,
    /// atalho de arquivo (reparse) e DACL gravável são ignorados; o legítimo é lido.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_n2_leitura_por_handle_ignora_forjado_link_e_dacl_gravavel() {
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        let d = ensure_dir().unwrap();
        write_state(b"{\"legit\":true}").unwrap();
        let st = d.join(STATE_FILE);
        assert_eq!(
            read_state().unwrap().as_deref(),
            Some(&b"{\"legit\":true}"[..])
        );
        // 1) forjado de outro dono no lugar do arquivo
        std::fs::remove_file(&st).unwrap();
        std::fs::write(&st, b"{\"forjado\":true}").unwrap();
        imp::test_set_owner(&st, "O:BU").unwrap();
        assert!(read_state().unwrap().is_none(), "dono Usuários");
        // 2) dono Administradores, mas DACL com escrita para Usuários
        std::fs::remove_file(&st).unwrap();
        std::fs::write(&st, b"{\"forjado\":true}").unwrap();
        let o = std::process::Command::new("icacls")
            .arg(&st)
            .args([
                "/inheritance:r",
                "/grant",
                "Users:(M)",
                "Administrators:(F)",
                "SYSTEM:(F)",
            ])
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        assert!(read_state().unwrap().is_none(), "DACL gravável");
        // 3) atalho (link simbólico de arquivo) para um arquivo legítimo: o link não é seguido
        let alvo = d.join("alvo.json");
        write_state(b"{\"legit\":true}").unwrap();
        std::fs::rename(&st, &alvo).unwrap();
        let o = std::process::Command::new("cmd")
            .args(["/C", "mklink"])
            .arg(&st)
            .arg(&alvo)
            .output()
            .unwrap();
        assert!(o.status.success(), "mklink: {o:?}");
        assert!(read_state().unwrap().is_none(), "link de arquivo");
        std::fs::remove_file(&st).unwrap();
        limpar_real(&dir);
    }

    /// N2: com o `state.json` aberto pelo leitor, a conta comum não consegue renomeá-lo nem
    /// apagá-lo (sem `FILE_SHARE_DELETE`). Segura uma abertura igual à do leitor e tenta.
    #[test]
    #[ignore = "cria %ProgramData%\\GuiaLar de verdade (administrador, VM)"]
    fn acl_real_n2_arquivo_aberto_pelo_leitor_nao_pode_ser_renomeado_nem_apagado() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = state_dir().unwrap();
        limpar_real(&dir);
        let d = ensure_dir().unwrap();
        write_state(b"{}").unwrap();
        let st = d.join(STATE_FILE);
        // mesmos direitos de compartilhamento do `read_trusted_file`: só FILE_SHARE_READ
        let aberto = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&st)
            .unwrap();
        assert!(std::fs::rename(&st, d.join("outro.json")).is_err());
        assert!(std::fs::remove_file(&st).is_err());
        drop(aberto);
        std::fs::rename(&st, d.join("outro.json")).expect("depois de fechar, renomeia");
        limpar_real(&dir);
    }
}

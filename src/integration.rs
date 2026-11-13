//! Integração silenciosa com o menu de aplicativos do Linux (ADR-021).
//!
//! No primeiro `gui`/`apply`: grava `~/.local/share/applications/guialar-digital.desktop`
//! apontando para o AppImage (ou binário) e instala o ícone. Nunca falha o programa.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::path::{Path, PathBuf};

pub const APP_ID: &str = "br.uniube.pi.guialar";
const ICON_PNG: &[u8] = include_bytes!("../assets/guialar.png");

/// Escapa um caminho para o campo `Exec=` (aspas duplas; `"`, `\`, `` ` `` e `$` com barra).
pub fn exec_quote(path: &str) -> String {
    let mut out = String::from("\"");
    for c in path.chars() {
        if matches!(c, '"' | '\\' | '`' | '$') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Conteúdo do `.desktop`.
pub fn desktop_entry(exec_path: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=GuiaLar Digital\nComment=Proteção familiar de navegação e bloqueador de anúncios\nExec={} gui\nIcon={}\nTerminal=false\nCategories=Network;Security;\nStartupNotify=true\nStartupWMClass={}\nX-GUILAR-AUTO=1\n",
        exec_quote(exec_path),
        APP_ID,
        APP_ID
    )
}

/// Caminho a gravar em `Exec=`: o AppImage, ou o binário se não estiver num mount FUSE.
pub fn target_exec(appimage: Option<&str>, current_exe: &Path) -> Option<String> {
    if let Some(a) = appimage.filter(|a| !a.is_empty()) {
        return Some(a.to_string());
    }
    let exe = current_exe.to_string_lossy().to_string();
    (!exe.contains("/.mount_")).then_some(exe)
}

fn write_if_changed(path: &Path, content: &[u8]) -> std::io::Result<bool> {
    if std::fs::read(path).map(|c| c == content).unwrap_or(false) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, content)?;
    Ok(true)
}

/// Instala `.desktop` e ícone sob `data_home` (`~/.local/share`). Devolve o `.desktop` se mudou.
///
/// `overwrite`: só o AppImage em execução pode reescrever o atalho que já existe (o `Exec`
/// aponta para ele mesmo). Um binário de outra pasta (build, `/tmp`) só cria o atalho quando
/// ainda não há nenhum, e nunca troca o do usuário (ADR-028).
pub fn install_into(
    data_home: &Path,
    exec_path: &str,
    overwrite: bool,
) -> std::io::Result<Option<PathBuf>> {
    let desktop = data_home
        .join("applications")
        .join("guialar-digital.desktop");
    let icon = data_home
        .join("icons/hicolor/256x256/apps")
        .join(format!("{APP_ID}.png"));
    write_if_changed(&icon, ICON_PNG)?;
    if !overwrite && desktop.exists() {
        return Ok(None);
    }
    let changed = write_if_changed(&desktop, desktop_entry(exec_path).as_bytes())?;
    Ok(changed.then_some(desktop))
}

/// Chamada no `gui` e no `apply` (Linux, usuário comum). Silenciosa: ignora qualquer falha.
#[cfg(target_os = "linux")]
pub fn ensure_desktop_integration() {
    if crate::sysutil::is_root() || std::env::var_os("GUIALAR_NO_INTEGRATION").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // `APPIMAGE` só vale se este executável está dentro do `APPDIR` (ADR-028, rev. 2): um
    // processo filho de OUTRO AppImage herda a variável e não pode trocar o atalho.
    let appimage = crate::sysutil::running_appimage();
    let appimage_str = appimage.as_ref().map(|p| p.to_string_lossy().into_owned());
    let Some(exec_path) = target_exec(appimage_str.as_deref(), &exe) else {
        return;
    };
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    let Some(data_home) = data_home else { return };
    if let Ok(Some(desktop)) = install_into(&data_home, &exec_path, appimage.is_some()) {
        let quiet = |mut c: std::process::Command| {
            let _ = c
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        };
        if crate::sysutil::command_exists("xdg-desktop-menu") {
            let mut c = std::process::Command::new("xdg-desktop-menu");
            c.args(["install", "--novendor"]).arg(&desktop);
            quiet(c);
        }
        if crate::sysutil::command_exists("update-desktop-database") {
            let mut c = std::process::Command::new("update-desktop-database");
            c.arg(data_home.join("applications"));
            quiet(c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_com_espaco_e_aspas() {
        assert_eq!(
            exec_quote("/home/a b/GuiaLar.AppImage"),
            "\"/home/a b/GuiaLar.AppImage\""
        );
        assert_eq!(exec_quote("/x/\"y$z"), "\"/x/\\\"y\\$z\"");
    }

    #[test]
    fn desktop_tem_exec_icone_e_marca() {
        let d = desktop_entry("/opt/GuiaLar.AppImage");
        assert!(d.contains("Exec=\"/opt/GuiaLar.AppImage\" gui\n"));
        assert!(d.contains("Icon=br.uniube.pi.guialar\n"));
        assert!(d.contains("X-GUILAR-AUTO=1"));
    }

    #[test]
    fn alvo_prefere_appimage_e_ignora_mount_fuse() {
        let exe = Path::new("/tmp/.mount_GuiaLaXYZ/usr/bin/guialar");
        assert_eq!(
            target_exec(Some("/h/a.AppImage"), exe).as_deref(),
            Some("/h/a.AppImage")
        );
        assert_eq!(target_exec(None, exe), None);
        assert_eq!(
            target_exec(None, Path::new("/usr/bin/guialar")).as_deref(),
            Some("/usr/bin/guialar")
        );
    }

    fn tmp(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gl-integ-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn exec_do_atalho(d: &Path) -> String {
        std::fs::read_to_string(d.join("applications/guialar-digital.desktop"))
            .unwrap()
            .lines()
            .find(|l| l.starts_with("Exec="))
            .unwrap()
            .to_string()
    }

    #[test]
    fn instala_idempotente() {
        let d = tmp("idem");
        assert!(install_into(&d, "/a/b", true).unwrap().is_some());
        assert!(install_into(&d, "/a/b", true).unwrap().is_none()); // sem duplicar
        assert!(install_into(&d, "/c/d", true).unwrap().is_some()); // caminho mudou: reescreve
        assert!(d
            .join("icons/hicolor/256x256/apps/br.uniube.pi.guialar.png")
            .exists());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn binario_de_outra_pasta_nao_reescreve_o_atalho_do_appimage() {
        let d = tmp("outra-pasta");
        // o AppImage instala o atalho
        install_into(&d, "/home/u/GuiaLar.AppImage", true).unwrap();
        // um binário de build (sem APPIMAGE) roda: não troca o atalho
        assert!(install_into(&d, "/home/u/gl/target/release/guialar", false)
            .unwrap()
            .is_none());
        assert_eq!(
            exec_do_atalho(&d),
            "Exec=\"/home/u/GuiaLar.AppImage\" gui",
            "o atalho continua no AppImage"
        );
        // o AppImage, mudado de lugar, reescreve
        install_into(&d, "/home/u/Downloads/GuiaLar.AppImage", true).unwrap();
        assert!(exec_do_atalho(&d).contains("Downloads/GuiaLar.AppImage"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn binario_sem_appimage_cria_o_atalho_quando_nao_existe() {
        let d = tmp("cria");
        assert!(install_into(&d, "/usr/local/bin/guialar", false)
            .unwrap()
            .is_some());
        assert_eq!(exec_do_atalho(&d), "Exec=\"/usr/local/bin/guialar\" gui");
        // um segundo binário, de outro lugar, não o troca
        assert!(install_into(&d, "/tmp/guialar", false).unwrap().is_none());
        assert_eq!(exec_do_atalho(&d), "Exec=\"/usr/local/bin/guialar\" gui");
        std::fs::remove_dir_all(&d).ok();
    }

    /// `APPIMAGE` herdado de OUTRO AppImage (terminal de um editor, por exemplo) não pode
    /// trocar o atalho do usuário: o executável não está no `APPDIR` dele (ADR-028, rev. 2).
    #[cfg(target_os = "linux")]
    #[test]
    fn appimage_herdado_de_outro_nao_reescreve_o_atalho() {
        let s = crate::testenv::Sandbox::new("integ-herdado");
        let data = s.dir().join(".local/share");
        std::env::set_var("XDG_DATA_HOME", &data);
        install_into(&data, "/home/u/GuiaLar.AppImage", true).unwrap();
        std::env::set_var("APPIMAGE", "/home/u/Outro.AppImage");
        std::env::set_var("APPDIR", "/tmp/.mount_OutroXYZ");
        ensure_desktop_integration();
        std::env::remove_var("APPIMAGE");
        std::env::remove_var("APPDIR");
        std::env::remove_var("XDG_DATA_HOME");
        let exec =
            std::fs::read_to_string(data.join("applications/guialar-digital.desktop")).unwrap();
        assert!(exec.contains("GuiaLar.AppImage"), "{exec}");
        assert!(!exec.contains("Outro.AppImage"), "{exec}");
    }
}

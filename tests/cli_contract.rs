//! QA: testes de contrato black-box da CLI (executa o binário compilado; não toca produção).

use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_guialar"));
    c.env("GUIALAR_NO_INTEGRATION", "1");
    c
}

#[test]
fn help_lists_the_four_public_subcommands() {
    let out = bin().arg("--help").output().expect("executa --help");
    assert!(out.status.success(), "--help deve sair 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["status", "apply", "revert", "browser"] {
        assert!(stdout.contains(sub), "--help deve listar '{}'", sub);
    }
}

#[test]
fn version_reports_cargo_version() {
    let out = bin().arg("--version").output().expect("executa --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "versão inesperada: {}",
        stdout.trim()
    );
}

#[test]
fn status_exits_success_and_reports_state() {
    let out = bin().arg("status").output().expect("executa status");
    assert!(out.status.success(), "status deve sair 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Estado:"), "saída deve conter 'Estado:'");
}

#[test]
fn default_command_is_status() {
    // Sem display, o comando padrão cai no resumo (com display abriria a janela).
    let out = bin()
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("executa sem argumentos");
    assert!(out.status.success(), "comando default deve sair 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Estado:"),
        "default deve reportar 'Estado:'"
    );
}

#[test]
fn unknown_subcommand_is_rejected() {
    let out = bin()
        .arg("comando-inexistente")
        .output()
        .expect("executa comando inválido");
    assert!(!out.status.success(), "subcomando desconhecido deve falhar");
}

/// Regressão do achado do QA (T10): segundo `apply` não pode tocar no sistema nem
/// sobrescrever o manifesto com o DNS já filtrado como se fosse o original.
#[cfg(unix)]
#[test]
fn apply_twice_keeps_original_manifest_untouched() {
    let home = std::env::temp_dir().join(format!("guialar-dup-{}", std::process::id()));
    let dir = home.join(".config").join("guialar");
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = r#"{"backend":"DirectResolvConf","applied_at_utc":"1","connections":[],"previous_resolv_conf":"nameserver 9.9.9.9\n"}"#;
    std::fs::write(dir.join("state.json"), manifest).unwrap();

    let out = bin()
        .arg("apply")
        .env("HOME", &home)
        // DNS "configurado" fixo (só em build de depuração): o do modo família, sem ler a
        // rede do host. Com DNS de fora o apply reaplicaria em vez de dizer "já está ativa".
        .env(
            "GUIALAR_TEST_DNS",
            "1.1.1.3,1.0.0.3,2606:4700:4700::1113,2606:4700:4700::1003",
        )
        .env_remove("SUDO_USER")
        .output()
        .expect("executa apply");

    let after = std::fs::read_to_string(dir.join("state.json")).unwrap();
    std::fs::remove_dir_all(&home).ok();
    assert!(out.status.success(), "apply duplicado deve sair 0");
    assert!(String::from_utf8_lossy(&out.stdout).contains("já está ativa"));
    assert_eq!(after, manifest, "manifesto original não pode mudar");
}

#[test]
fn gui_without_display_fails_cleanly() {
    let out = bin()
        .arg("gui")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("executa gui");
    assert!(!out.status.success(), "gui sem display deve falhar");
    assert!(String::from_utf8_lossy(&out.stderr).contains("ambiente gráfico"));
}

#[test]
fn apply_mode_option_is_documented_and_validated() {
    let out = bin()
        .args(["apply", "--help"])
        .output()
        .expect("apply --help");
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--mode"), "help deve citar --mode: {help}");
    assert!(help.contains("family") && help.contains("ads"));

    let bad = bin()
        .args(["apply", "--mode", "banana"])
        .output()
        .expect("modo inválido");
    assert_eq!(bad.status.code(), Some(2), "modo inválido deve sair 2");
}

#[test]
fn apply_mode_ads_alias_is_accepted_by_parser() {
    // `--help` curto-circuita antes de tocar no sistema: só valida o parser.
    for m in ["family", "ads", "ads-only"] {
        let out = bin()
            .args(["apply", "--mode", m, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success(), "--mode {m} deve ser aceito");
    }
}

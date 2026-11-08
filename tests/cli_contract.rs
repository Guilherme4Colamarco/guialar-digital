//! QA: testes de contrato black-box da CLI (executa o binário compilado; não toca produção).

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_guialar"))
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
fn version_reports_0_2_0() {
    let out = bin().arg("--version").output().expect("executa --version");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("0.2.0"),
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
    let out = bin().output().expect("executa sem argumentos");
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

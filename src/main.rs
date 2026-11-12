//! Ponto de entrada do GuiaLar Digital: janela nativa (sem argumentos) ou CLI.
//!
//! No Windows o binário usa o subsistema "windows" (duplo clique não abre console);
//! com argumentos ele se anexa ao console de quem o chamou.
#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod browser;
mod config;
mod integration;
mod platform;
mod policy;
mod safefs;
mod statedir;
mod sysutil;
#[cfg(test)]
mod testenv;
mod ui;
mod verify;
#[cfg(windows)]
mod win32;

use crate::config::DnsStateManifest;
use crate::verify::ProtectionStatus;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "guialar",
    version,
    about = "GuiaLar Digital - proteção de navegação via DNS filtrado",
    long_about = "Configura o DNS filtrado do computador: proteção familiar (Cloudflare for Families, bloqueia malware e conteúdo adulto) ou só anúncios (AdGuard DNS), e ajuda a instalar bloqueadores de anúncios. Funciona em Linux (NetworkManager, systemd-resolved, runit/resolvconf) e Windows."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Mostra se este computador está protegido e qual filtro está ativo.
    Status,
    /// Abre a janela gráfica (padrão quando não há argumentos).
    Gui,
    /// Tudo de uma vez: filtro de DNS + (no modo família) bloqueador de anúncios.
    Setup {
        /// Filtro: `family` (padrão, malware + adulto) ou `ads` (só anúncios e rastreadores).
        #[arg(long, value_enum, default_value_t = config::FilterMode::Family)]
        mode: config::FilterMode,
    },
    /// Ativa só o filtro de DNS (sem mexer nos navegadores).
    Apply {
        /// Filtro: `family` (padrão, malware + adulto) ou `ads` (só anúncios e rastreadores).
        #[arg(long, value_enum, default_value_t = config::FilterMode::Family)]
        mode: config::FilterMode,
    },
    /// Desfaz a proteção e devolve o DNS ao padrão da rede.
    Revert,
    /// Instala o bloqueador de anúncios nos navegadores (modo família).
    Browser {
        #[arg(long, short)]
        install_all: bool,
    },
    #[command(hide = true)]
    InternalApply {
        #[arg(long, value_enum, default_value_t = config::FilterMode::Family)]
        mode: config::FilterMode,
    },
    #[command(hide = true)]
    InternalRevert,
    #[command(hide = true)]
    InternalBrowser,
    #[command(hide = true)]
    InternalSetup {
        #[arg(long, value_enum, default_value_t = config::FilterMode::Family)]
        mode: config::FilterMode,
    },
}

fn main() -> ExitCode {
    #[cfg(windows)]
    if std::env::args_os().len() > 1 {
        win32::attach_parent_console();
    }
    let cli = Cli::parse();

    let sem_comando = cli.command.is_none();
    #[cfg(target_os = "linux")]
    if matches!(
        cli.command,
        None | Some(Commands::Gui) | Some(Commands::Apply { .. }) | Some(Commands::Setup { .. })
    ) {
        integration::ensure_desktop_integration();
    }
    if matches!(cli.command, None | Some(Commands::Gui)) {
        match ui::run() {
            Ok(true) => return ExitCode::SUCCESS,
            Ok(false) if sem_comando => {} // sem display: cai no resumo da CLI
            Ok(false) => {
                eprintln!("❌ Nenhum ambiente gráfico disponível (DISPLAY/WAYLAND_DISPLAY).");
                return ExitCode::FAILURE;
            }
            Err(err) => {
                eprintln!("\n❌ Erro ao abrir a janela: {:#}", err);
                #[cfg(windows)]
                win32::message_box(
                    &format!("Não foi possível abrir a janela do GuiaLar Digital.\n\n{err:#}"),
                    "GuiaLar Digital",
                    0x10,
                );
                if !sem_comando {
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    let result = match cli.command.unwrap_or(Commands::Status) {
        Commands::Status | Commands::Gui => handle_status(),
        Commands::Setup { mode } => {
            config::set_mode(mode);
            handle_setup(false)
        }
        Commands::InternalBrowser => handle_internal_browser(),
        Commands::InternalSetup { mode } => {
            config::set_mode(mode);
            handle_internal_setup()
        }
        Commands::Apply { mode } => {
            config::set_mode(mode);
            handle_apply()
        }
        Commands::Revert => handle_revert(),
        Commands::Browser { install_all } => handle_browser(install_all),
        Commands::InternalApply { mode } => {
            config::set_mode(mode);
            handle_internal_apply()
        }
        Commands::InternalRevert => handle_internal_revert(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("\n❌ Erro: {:#}", err);
            ExitCode::FAILURE
        }
    }
}

fn handle_setup(interativo: bool) -> anyhow::Result<()> {
    use std::io::{BufRead, Write};
    println!("🛡️  GuiaLar Digital — Configuração rápida\n");
    if config::current_mode() == config::FilterMode::AdsOnly {
        println!("Isto vai usar o DNS AdGuard (bloqueia só anúncios e rastreadores). Os navegadores não serão alterados.\n");
    } else {
        println!("Isto vai:\n  1. usar o DNS Cloudflare Family (bloqueia malware e conteúdo adulto)\n  2. instalar o bloqueador de anúncios uBlock nos navegadores encontrados\n");
    }
    let result = (|| {
        if interativo {
            print!("Continuar? [S/n] ");
            let _ = std::io::stdout().flush();
            let mut l = String::new();
            let _ = std::io::stdin().lock().read_line(&mut l);
            if matches!(l.trim().to_lowercase().as_str(), "n" | "nao" | "não") {
                println!("Nada foi alterado.");
                return Ok(());
            }
        }
        apply_flow(true)?;
        #[cfg(not(windows))]
        if config::current_mode() == config::FilterMode::Family {
            handle_browser(true)?;
        }
        println!("\n✅ Pronto. Reabra o navegador para o bloqueador aparecer.");
        println!("   Para desfazer o DNS: guialar revert");
        Ok(())
    })();
    if interativo {
        if let Err(e) = &result {
            eprintln!("\n❌ Erro: {e:#}");
        }
        print!("\nPressione Enter para fechar...");
        let _ = std::io::stdout().flush();
        let _ = std::io::stdin().lock().read_line(&mut String::new());
    }
    result
}

/// Linha de resultado do bloqueador: ✓ para o que foi feito, o próprio aviso (⚠️) para o que
/// foi pulado de propósito.
#[cfg(windows)]
fn print_install_line(line: &str) {
    if line.starts_with("⚠️") {
        println!("  {line}");
    } else {
        println!("  ✓ {line}");
    }
}

fn handle_internal_browser() -> anyhow::Result<()> {
    sysutil::require_privileged()?;
    let _lock = sysutil::OpLock::acquire()?;
    #[cfg(windows)]
    for line in policy::install_all()? {
        print_install_line(&line);
    }
    Ok(())
}

fn handle_status() -> anyhow::Result<()> {
    println!("🛡️  GuiaLar Digital — Status da Proteção\n");

    match verify::check_status() {
        ProtectionStatus::Protected => {
            println!("  Estado:  🟢 [PROTEGIDO]");
            let mode = verify::active_mode().unwrap_or_default();
            let [a, b] = mode.ipv4();
            let filtro = match mode {
                config::FilterMode::Family => "Cloudflare Family (Malware + Adulto)",
                config::FilterMode::AdsOnly => "AdGuard DNS (só anúncios e rastreadores)",
            };
            println!("  Filtro:  {filtro}");
            println!("  Modo:    {} ({})", mode.label(), mode.cli_name());
            println!("  IPs:     {a}, {b} e IPv6 correspondentes");
        }
        ProtectionStatus::PartiallyConfigured(msg) => {
            println!("  Estado:  🟡 [CONFIGURAÇÃO PARCIAL OU AVISO]");
            println!("  Aviso:   {}", msg);
        }
        ProtectionStatus::Unprotected => {
            println!("  Estado:  🔴 [NÃO PROTEGIDO]");
            println!("  Info:    O sistema está utilizando o DNS padrão da rede/provedor.");
            if platform::cloudflare_dns_active() {
                println!(
                    "  Aviso:   O filtro do GuiaLar ainda está na rede, mas não há registro dele."
                );
                println!("  Ação:    Execute 'guialar revert' para voltar ao DNS da rede.");
            } else {
                println!("  Ação:    Execute 'guialar apply' para ativar a proteção.");
            }
        }
    }

    println!("\n🌐 Navegadores detectados no sistema:");
    let browsers = browser::detect_browsers();
    let mut found = false;
    for b in browsers {
        if b.is_installed {
            found = true;
            println!(
                "  - {:<16} [Instalado] (Loja: {})",
                b.name, b.extension_store_url
            );
        }
    }
    if !found {
        println!("  - Nenhum navegador comum identificado automaticamente.");
    }

    Ok(())
}

fn handle_apply() -> anyhow::Result<()> {
    apply_flow(false)
}

/// `with_browsers`: no Windows faz DNS + navegadores com um único UAC.
fn apply_flow(with_browsers: bool) -> anyhow::Result<()> {
    let _guard = sysutil::guard_operation()?;
    if let Some(m) = DnsStateManifest::load()? {
        let wanted = config::current_mode();
        match platform::plan_apply(Some(&m), wanted, &platform::configured_dns()) {
            platform::ApplyPlan::AlreadyActive => {
                println!(
                    "ℹ️  A proteção já está ativa. Use 'guialar revert' antes de aplicar de novo."
                );
                return Ok(());
            }
            _ if m.mode != wanted => {
                println!("🔁 Trocando o filtro para: {}", wanted.label());
            }
            _ => {
                println!("🔁 O DNS deste computador não é o do GuiaLar: reaplicando o filtro...");
            }
        }
    }
    println!(
        "⚙️  Aplicando \"{}\" no sistema...",
        config::current_mode().label()
    );
    if with_browsers {
        platform::apply_all()?;
    } else {
        platform::apply_dns()?;
    }
    println!("✅ {} ativada com sucesso!", config::current_mode().label());
    println!("🔍 Validando conexão com smoke test...");

    match verify::check_status() {
        ProtectionStatus::Protected => {
            println!("🎉 Conexão validada: A rede já está navegando com o filtro ativo.");
        }
        ProtectionStatus::PartiallyConfigured(msg) => {
            eprintln!("⚠️  Aviso pós-aplicação: {}", msg);
        }
        ProtectionStatus::Unprotected => {
            eprintln!("⚠️  O DNS foi gravado, mas a verificação direta ainda reporta estado não protegido.");
        }
    }

    Ok(())
}

/// Reverte a proteção. Cobre manifesto normal, ausente e corrompido (nunca deixa o
/// DNS preso no filtro). Devolve a mensagem final.
fn revert_flow() -> anyhow::Result<String> {
    match DnsStateManifest::load() {
        Ok(Some(manifest)) => {
            println!("🔄 Restaurando configurações originais de DNS...");
            platform::revert_dns(&manifest)?;
            Ok("Configurações de DNS restauradas com sucesso para o estado anterior.".into())
        }
        Ok(None) => {
            // Beta: o manifesto ficava na área do usuário. O processo elevado o valida e o
            // migra uma vez; nunca mais é gravado ali (ADR-031).
            if sysutil::is_privileged() {
                if let Some(msg) = platform::migrate_legacy()? {
                    return Ok(msg);
                }
            }
            if platform::cloudflare_dns_active() {
                println!("🔄 O DNS ainda usa o filtro, mas não há registro: voltando ao padrão da rede...");
                platform::revert_dns_fallback()?;
                return Ok("DNS restaurado para o padrão da rede.".into());
            }
            #[cfg(windows)]
            {
                // Sem DNS para reverter, mas ainda pode haver políticas do uBlock.
                if platform::elevate_if_needed("internal-revert")? {
                    return Ok(
                        "Nenhum manifesto de proteção ativo encontrado para reverter.".into(),
                    );
                }
                policy::revert_all()?;
            }
            Ok("Nenhum manifesto de proteção ativo encontrado para reverter.".into())
        }
        // Falha de E/S ao LER (arquivo aberto por outro programa): o registro pode estar
        // perfeito. Não o apaga nem cai no fallback; avisa e para sem alterar nada.
        Err(e) if statedir::is_state_io_error(&e) => {
            eprintln!(
                "⚠️  {e:#}. Nada foi alterado; feche o programa que mantém o registro aberto e rode o desfazer de novo."
            );
            Err(e)
        }
        Err(_) => {
            println!(
                "⚠️  O registro da proteção está danificado: voltando o DNS ao padrão da rede..."
            );
            if let Ok(path) = DnsStateManifest::file_path() {
                let _ = std::fs::remove_file(path);
            }
            platform::revert_dns_fallback()?;
            Ok("O registro estava danificado. DNS restaurado para o padrão da rede.".into())
        }
    }
}

fn handle_revert() -> anyhow::Result<()> {
    let _guard = sysutil::guard_operation()?;
    let msg = revert_flow()?;
    let icone = if msg.starts_with("Nenhum") {
        "ℹ️ "
    } else {
        "✅"
    };
    println!("{icone} {msg}");
    Ok(())
}

fn handle_browser(install_all: bool) -> anyhow::Result<()> {
    println!("🧩 uBlock Origin — Proteção de Anúncios e Rastreamento\n");
    #[cfg(windows)]
    {
        let _ = install_all;
        if platform::elevate_if_needed("internal-browser")? {
            return Ok(());
        }
        let feitos = policy::install_all()?;
        if feitos.is_empty() {
            anyhow::bail!("Nenhum navegador compatível (Chrome, Edge, Brave, Firefox) encontrado.");
        }
        for line in feitos {
            print_install_line(&line);
        }
        Ok(())
    }
    #[cfg(not(windows))]
    open_store_pages(install_all)
}

#[cfg(not(windows))]
fn open_store_pages(install_all: bool) -> anyhow::Result<()> {
    let browsers = browser::detect_browsers();
    let mut opened = false;

    for b in &browsers {
        if b.is_installed {
            println!("Abrindo página de instalação para {}...", b.name);
            if browser::open_extension_page(b.extension_store_url).is_err() {
                anyhow::bail!(
                    "Não consegui abrir o navegador. Abra manualmente: {}",
                    b.extension_store_url
                );
            }
            opened = true;
            if !install_all {
                break;
            }
        }
    }
    if !opened {
        anyhow::bail!(
            "Nenhum navegador compatível encontrado (Firefox, Chrome, Brave, Chromium, Edge)."
        );
    }
    Ok(())
}

fn handle_internal_setup() -> anyhow::Result<()> {
    sysutil::require_privileged()?;
    let _lock = sysutil::OpLock::acquire()?;
    platform::apply_all()
}

fn handle_internal_apply() -> anyhow::Result<()> {
    sysutil::require_privileged()?;
    let _lock = sysutil::OpLock::acquire()?;
    platform::apply_dns()
}

fn handle_internal_revert() -> anyhow::Result<()> {
    sysutil::require_privileged()?;
    let _lock = sysutil::OpLock::acquire()?;
    revert_flow().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli_grammar() {
        Cli::command().debug_assert();
    }

    /// B3: só «arquivo ocupado por outro programa» (Windows) é erro de E/S marcado e poupa o
    /// registro. Uma leitura negada por outro motivo (aqui, sem permissão no Linux) volta ao
    /// caminho de antes: o `load` falha sem a marca e o `revert_flow` cai no fallback, que
    /// nunca deixa o DNS preso no filtro.
    #[cfg(target_os = "linux")]
    #[test]
    fn leitura_negada_no_linux_nao_e_erro_de_es_marcado() {
        use std::os::unix::fs::PermissionsExt;
        if sysutil::is_root() {
            return; // como root a leitura nunca é negada
        }
        let s = testenv::Sandbox::new("revert-io");
        statedir::write_state(b"{}").unwrap();
        let f = statedir::state_file().unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o000)).unwrap();
        let e = DnsStateManifest::load().unwrap_err();
        assert!(!statedir::is_state_io_error(&e), "{e:#}");
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        drop(s);
    }
}

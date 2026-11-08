//! Ponto de entrada CLI do GuiaLar Digital em Rust.

mod browser;
mod config;
mod platform;
mod policy;
mod sysutil;
mod verify;

use crate::config::DnsStateManifest;
use crate::verify::ProtectionStatus;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "guialar",
    version = "0.2.1",
    about = "GuiaLar Digital - Proteção familiar de navegação via DNS Cloudflare Family e uBlock Origin",
    long_about = "Configura de forma segura o DNS da Cloudflare for Families (1.1.1.3 / 1.0.0.3) e auxilia na instalação de bloqueadores de anúncios em Linux (NetworkManager, systemd-resolved, runit/resolvconf) e Windows."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Status,
    /// Tudo de uma vez: DNS Cloudflare Family + bloqueador de anúncios.
    Setup,
    Apply,
    Revert,
    Browser {
        #[arg(long, short)]
        install_all: bool,
    },
    #[command(hide = true)]
    InternalApply,
    #[command(hide = true)]
    InternalRevert,
    #[command(hide = true)]
    InternalBrowser,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let sem_comando = cli.command.is_none();
    // Duplo clique no .exe (sem argumentos) no Windows: assistente guiado.
    let default = if cfg!(windows) {
        Commands::Setup
    } else {
        Commands::Status
    };
    let result = match cli.command.unwrap_or(default) {
        Commands::Status => handle_status(),
        Commands::Setup => handle_setup(sem_comando),
        Commands::InternalBrowser => handle_internal_browser(),
        Commands::Apply => handle_apply(),
        Commands::Revert => handle_revert(),
        Commands::Browser { install_all } => handle_browser(install_all),
        Commands::InternalApply => handle_internal_apply(),
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
    println!("Isto vai:\n  1. usar o DNS Cloudflare Family (bloqueia malware e conteúdo adulto)\n  2. instalar o bloqueador de anúncios uBlock nos navegadores encontrados\n");
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
        handle_apply()?;
        handle_browser(true)?;
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

fn handle_internal_browser() -> anyhow::Result<()> {
    #[cfg(windows)]
    for line in policy::install_all()? {
        println!("  ✓ {line}");
    }
    Ok(())
}

fn handle_status() -> anyhow::Result<()> {
    println!("🛡️  GuiaLar Digital — Status da Proteção Familiar\n");

    match verify::check_status() {
        ProtectionStatus::Protected => {
            println!("  Estado:  🟢 [PROTEGIDO]");
            println!("  Filtro:  Cloudflare Family (Malware + Adulto)");
            println!("  IPs:     1.1.1.3, 1.0.0.3 e IPv6 correspondentes");
        }
        ProtectionStatus::PartiallyConfigured(msg) => {
            println!("  Estado:  🟡 [CONFIGURAÇÃO PARCIAL OU AVISO]");
            println!("  Aviso:   {}", msg);
        }
        ProtectionStatus::Unprotected => {
            println!("  Estado:  🔴 [NÃO PROTEGIDO]");
            println!("  Info:    O sistema está utilizando o DNS padrão da rede/provedor.");
            println!("  Ação:    Execute 'guialar apply' para ativar a proteção.");
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
    println!("⚙️  Aplicando proteção familiar no sistema...");
    platform::apply_dns()?;
    println!("✅ Proteção familiar ativada com sucesso!");
    println!("🔍 Validando conexão com smoke test...");

    match verify::check_status() {
        ProtectionStatus::Protected => {
            println!(
                "🎉 Conexão validada: A rede já está navegando com filtros familiares ativos."
            );
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

fn handle_revert() -> anyhow::Result<()> {
    println!("🔄 Restaurando configurações originais de DNS...");

    let Some(manifest) = DnsStateManifest::load()? else {
        println!("ℹ️  Nenhum manifesto de proteção ativo encontrado para reverter.");
        return Ok(());
    };

    platform::revert_dns(&manifest)?;
    println!("✅ Configurações de DNS restauradas com sucesso para o estado anterior.");
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
            println!("  ✓ {line}");
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
            if let Err(e) = browser::open_extension_page(b.extension_store_url) {
                anyhow::bail!(
                    "Não foi possível abrir a página. Abra manualmente: {} ({e})",
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

fn handle_internal_apply() -> anyhow::Result<()> {
    platform::apply_dns()
}

fn handle_internal_revert() -> anyhow::Result<()> {
    if let Some(manifest) = DnsStateManifest::load()? {
        platform::revert_dns(&manifest)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn verify_cli_grammar() {
        Cli::command().debug_assert();
    }
}

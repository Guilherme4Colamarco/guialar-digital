//! Ponto de entrada CLI do GuiaLar Digital em Rust.

mod browser;
mod config;
mod platform;
mod sysutil;
mod verify;

use crate::config::DnsStateManifest;
use crate::verify::ProtectionStatus;
use clap::{Parser, Subcommand};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "guialar",
    version = "0.2.0",
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
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command.unwrap_or(Commands::Status) {
        Commands::Status => handle_status(),
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
    println!("🧩 Extensão uBlock Origin — Proteção de Anúncios e Rastreamento\n");
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

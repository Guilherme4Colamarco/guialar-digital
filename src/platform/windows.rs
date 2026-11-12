//! Backend Windows: DNS via PowerShell (`-EncodedCommand`) e elevação por UAC.

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;

use super::runner::{Cmd, CommandRunner, RealRunner};
use crate::config::{current_mode, BackendKind, ConnectionBackup, DnsStateManifest};
use crate::sysutil::{encode_powershell, is_safe_ps_alias};

/// Prefixo: saída UTF-8 e erro fatal (um cmdlet que falha deve falhar o processo).
const PS_PREFIX: &str =
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; $ErrorActionPreference='Stop'; ";

fn powershell(script: &str) -> Cmd {
    Cmd::output(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encode_powershell(&format!("{PS_PREFIX}{script}")),
        ],
    )
    .no_window() // CREATE_NO_WINDOW: nenhuma janela de console pisca
}

fn powershell_output(r: &dyn CommandRunner, script: &str) -> Result<String> {
    let out = r
        .run(&powershell(script))
        .context("Falha ao executar PowerShell")?;
    if !out.success() {
        bail!(
            "PowerShell falhou ({:?}): {}",
            out.code,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout_text())
}

fn run_powershell(r: &dyn CommandRunner, script: &str) -> Result<()> {
    powershell_output(r, script).map(|_| ())
}

fn is_admin() -> bool {
    // SAFETY: função Win32 sem argumentos.
    unsafe { crate::win32::IsUserAnAdmin() != 0 }
}

/// Se não é administrador, reabre o programa com UAC (sem janela) rodando
/// `subcomando` e espera terminar. `true` = trabalho feito na instância elevada.
pub fn elevate_if_needed(subcomando: &str) -> Result<bool> {
    if is_admin() {
        return Ok(false);
    }
    crate::sysutil::OpLock::check_free()?;
    let exe = std::env::current_exe().context("não foi possível localizar o executável")?;
    elevation_result(crate::win32::run_elevated_hidden(
        &exe.to_string_lossy(),
        subcomando,
    ))
}

/// Resultado do UAC: negado/timeout (`Err`) ou código de saída da instância elevada.
fn elevation_result(r: Result<u32, String>) -> Result<bool> {
    let code = r.map_err(anyhow::Error::msg)?;
    if code != 0 {
        bail!("a operação elevada falhou (código {code})");
    }
    Ok(true)
}

pub fn apply_dns() -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-apply --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    apply_core()
}

/// Troca o filtro mantendo o DNS original do manifesto (e tirando as políticas de navegador
/// quando o novo modo é só anúncios).
pub fn switch_mode(mut manifest: DnsStateManifest) -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-apply --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    let mode = current_mode();
    set_servers_on_connections(&RealRunner, &manifest, mode)?;
    if mode == crate::config::FilterMode::AdsOnly {
        crate::policy::revert_all()?;
    } else {
        crate::policy::install_all()?; // voltou ao familiar: o bloqueador de anúncios volta junto
    }
    manifest.mode = mode;
    manifest.save_atomic()?;
    flush_dns(&RealRunner);
    Ok(())
}

/// Troca o DNS de cada conexão do manifesto pelo do `mode` (alias por nome).
fn set_servers_on_connections(
    r: &dyn CommandRunner,
    manifest: &DnsStateManifest,
    mode: crate::config::FilterMode,
) -> Result<()> {
    let servers = format!(
        "'{}','{}','{}','{}'",
        mode.ipv4()[0],
        mode.ipv4()[1],
        mode.ipv6()[0],
        mode.ipv6()[1]
    );
    for conn in &manifest.connections {
        if !is_safe_ps_alias(&conn.id) {
            bail!("alias inválido no manifesto: {:?}", conn.id);
        }
        run_powershell(
            r,
            &format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ServerAddresses @({servers})",
                conn.id
            ),
        )?;
    }
    Ok(())
}

/// DNS + políticas dos navegadores com um único prompt de UAC.
pub fn apply_all() -> Result<()> {
    if elevate_if_needed(&format!(
        "internal-setup --mode {}",
        current_mode().cli_name()
    ))? {
        return Ok(());
    }
    apply_core()?;
    if current_mode() == crate::config::FilterMode::Family {
        crate::policy::install_all()?; // políticas de navegador só no modo familiar
    }
    Ok(())
}

/// Já elevado: grava o DNS e o manifesto.
fn apply_core() -> Result<()> {
    apply_core_with(&RealRunner)
}

fn apply_core_with(r: &dyn CommandRunner) -> Result<()> {
    let _lock = crate::sysutil::OpLock::acquire()?;
    // se o estado não puder ser guardado com segurança, nada do DNS é alterado (ADR-031)
    DnsStateManifest::preflight()?;
    let interfaces = list_active_interfaces(r)?;
    if interfaces.is_empty() {
        bail!("Nenhuma interface de rede ativa encontrada no Windows");
    }
    let servers = format!(
        "'{}','{}','{}','{}'",
        current_mode().ipv4()[0],
        current_mode().ipv4()[1],
        current_mode().ipv6()[0],
        current_mode().ipv6()[1]
    );
    // Primeiro INSPECIONA todas as interfaces; só depois altera alguma. Assim um erro de leitura
    // não deixa o DNS de umas trocado e as outras não, sem manifesto para desfazer.
    let mut manifest = DnsStateManifest::new(BackendKind::WindowsPowerShell);
    let mut alvos: Vec<u32> = Vec::new();
    for (alias, index) in &interfaces {
        if !is_safe_ps_alias(alias) {
            bail!("Alias de interface inválido retornado pelo Windows: {alias:?}");
        }
        match inspect_interface_state(r, *index, alias)? {
            Some(backup) => {
                manifest.connections.push(backup);
                alvos.push(*index);
            }
            // sem GUID não há como guardar o estado: não mexe nessa interface
            None => eprintln!(
                "⚠️ A interface {alias:?} foi ignorada: o Windows não informou um GUID válido, então o DNS dela não pode ser guardado nem restaurado."
            ),
        }
    }
    if alvos.is_empty() {
        bail!("Nenhuma interface de rede ativa pôde ser inspecionada no Windows");
    }
    // O manifesto descreve o estado de ANTES: grava-o antes de trocar qualquer DNS, para que uma
    // falha no meio do laço (adaptador que some, Wi-Fi que cai) nunca deixe DNS trocado sem
    // registro para desfazer. Se nem isso der (registro aberto por outro programa), para aqui.
    manifest.save_atomic().context("Nada foi alterado no DNS")?;
    for index in alvos {
        if let Err(e) = run_powershell(
            r,
            &format!(
                "Set-DnsClientServerAddress -InterfaceIndex {index} -ServerAddresses @({servers})"
            ),
        ) {
            // desfaz o que já foi trocado (restaurar uma interface intacta é inócuo); se o
            // desfazer funcionou, o registro não tem mais o que guardar
            return match restore_connections(r, &manifest) {
                Ok(()) => {
                    let _ = DnsStateManifest::remove_manifest();
                    Err(e.context("Falha ao trocar o DNS; o que já tinha sido trocado foi desfeito"))
                }
                Err(re) => Err(e.context(format!(
                    "Falha ao trocar o DNS e não consegui desfazer ({re:#}); use «Desativar» para restaurar"
                ))),
            };
        }
    }
    flush_dns(r);
    Ok(())
}

pub fn revert_dns(manifest: &DnsStateManifest) -> Result<()> {
    if elevate_if_needed("internal-revert")? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    revert_dns_with(&RealRunner, manifest, crate::policy::revert_all)
}

/// O desfazer completo, com as extensões injetáveis para teste. Ordem: DNS, extensões, cache e
/// só no fim apaga o registro. Se o registro não puder ser apagado (outra conta o mantém
/// aberto sem `FILE_SHARE_DELETE`), o DNS e as extensões já foram revertidos: só avisa.
fn revert_dns_with(
    r: &dyn CommandRunner,
    manifest: &DnsStateManifest,
    revert_extensions: impl FnOnce() -> Result<()>,
) -> Result<()> {
    restore_connections(r, manifest)?;
    revert_extensions()?;
    flush_dns(r);
    remove_manifest_or_warn();
    Ok(())
}

/// Apaga o registro; se não der, avisa com clareza em vez de falhar um desfazer já concluído.
fn remove_manifest_or_warn() {
    if let Err(e) = DnsStateManifest::remove_manifest() {
        eprintln!(
            "⚠️ O DNS e as extensões foram restaurados, mas o registro da proteção não pôde ser apagado ({e:#}). Provavelmente outro programa o mantém aberto: feche-o e rode o desfazer de novo."
        );
    }
}

/// Devolve cada interface ao DNS de antes (automático ou os IPs gravados).
fn restore_connections(r: &dyn CommandRunner, manifest: &DnsStateManifest) -> Result<()> {
    let mut falhas: Vec<String> = Vec::new();
    for conn in &manifest.connections {
        // O alias vem do manifesto em disco: pode ter sido adulterado.
        if !is_safe_ps_alias(&conn.id) {
            falhas.push(format!("alias inválido no manifesto: {:?}", conn.id));
            continue;
        }
        let static4 = !conn.was_dhcp_ipv4 && !conn.original_ipv4_dns.is_empty();
        let static6 = !conn.was_dhcp_ipv6 && !conn.original_ipv6_dns.is_empty();
        let script = if static4 || static6 {
            let ips: Vec<String> = conn
                .original_ipv4_dns
                .iter()
                .map(|ip| format!("'{ip}'"))
                .chain(conn.original_ipv6_dns.iter().map(|ip| format!("'{ip}'")))
                .collect();
            // ResetServerAddresses antes: senão o DNS da Cloudflare ficaria na interface.
            // A família que era DHCP fica automática (o reset a limpou).
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{0}' -ResetServerAddresses; Set-DnsClientServerAddress -InterfaceAlias '{0}' -ServerAddresses @({1})",
                conn.id,
                ips.join(",")
            )
        } else if conn.was_dhcp_ipv4 || conn.was_dhcp_ipv6 {
            format!(
                "Set-DnsClientServerAddress -InterfaceAlias '{}' -ResetServerAddresses",
                conn.id
            )
        } else {
            continue;
        };
        if let Err(e) = run_powershell(r, &script) {
            falhas.push(format!("{}: {e:#}", conn.id));
        }
    }
    if !falhas.is_empty() {
        bail!(
            "Falha ao reverter DNS em {} interface(s): {}",
            falhas.len(),
            falhas.join(" | ")
        );
    }
    Ok(())
}

/// Aliases das interfaces de rede de agora (para validar o manifesto antigo).
pub fn known_connection_ids() -> Vec<String> {
    list_active_interfaces(&RealRunner)
        .map(|v| v.into_iter().map(|(alias, _)| alias).collect())
        .unwrap_or_default()
}

/// Migração do manifesto antigo da beta (ADR-031); ver `platform::migrate_legacy`.
pub fn migrate_legacy() -> Result<Option<String>> {
    super::migrate_legacy_generic(
        BackendKind::WindowsPowerShell,
        &known_connection_ids(),
        revert_dns,
        revert_dns_fallback,
    )
}

fn list_active_interfaces(r: &dyn CommandRunner) -> Result<Vec<(String, u32)>> {
    let out = powershell_output(
        r,
        "Get-NetIPInterface -AddressFamily IPv4 -ConnectionState Connected | Where-Object { $_.InterfaceAlias -notmatch 'Loopback|vEthernet' } | Sort-Object InterfaceIndex -Unique | ForEach-Object { \"$($_.InterfaceAlias)|$($_.InterfaceIndex)\" }",
    )?;
    Ok(out
        .lines()
        .filter_map(|l| {
            let (alias, idx) = l.trim().rsplit_once('|')?;
            Some((alias.to_string(), idx.parse().ok()?))
        })
        .collect())
}

/// GUID de adaptador (`{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}`): só isso entra no caminho do
/// registro (nada vindo do sistema é interpolado sem conferir).
fn is_adapter_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 38
        && b[0] == b'{'
        && b[37] == b'}'
        && s[1..37].chars().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

/// `NameServer` do registro da interface (`family` = `Tcpip` ou `Tcpip6`): o DNS **estático**
/// configurado à mão. Vazio quando a interface usa o DNS do DHCP. O DNS efetivo
/// (`Get-DnsClientServerAddress`) mistura os dois e não serve para decidir isso.
fn read_name_server(r: &dyn CommandRunner, guid: &str, family: &str) -> Result<String> {
    powershell_output(
        r,
        &format!(
            "(Get-ItemProperty -LiteralPath 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\{family}\\Parameters\\Interfaces\\{guid}' -Name NameServer -ErrorAction SilentlyContinue).NameServer"
        ),
    )
}

/// IPs de um `NameServer` (separados por vírgula, espaço ou ponto e vírgula).
fn parse_name_server<T: std::str::FromStr>(text: &str) -> Vec<T> {
    text.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter_map(|t| t.trim().parse().ok())
        .collect()
}

fn inspect_interface_state(
    r: &dyn CommandRunner,
    index: u32,
    alias: &str,
) -> Result<Option<ConnectionBackup>> {
    let guid = powershell_output(
        r,
        &format!("(Get-NetAdapter -InterfaceIndex {index}).InterfaceGuid"),
    )
    .context("Falha ao consultar o adaptador no Windows")?;
    let guid = guid.trim();
    if !is_adapter_guid(guid) {
        return Ok(None);
    }
    let ns4 =
        read_name_server(r, guid, "Tcpip").context("Falha ao consultar DNS atual no Windows")?;
    // IPv6 é melhor esforço: sem a chave (IPv6 desligado) vale «DHCP, sem fixos»
    let ns6 = read_name_server(r, guid, "Tcpip6").unwrap_or_else(|e| {
        eprintln!("⚠️ Não consegui ler o DNS IPv6 de {alias:?} ({e:#}); ao desativar, ele volta ao automático.");
        String::new()
    });
    let ips4: Vec<Ipv4Addr> = parse_name_server(&ns4);
    let ips6: Vec<std::net::Ipv6Addr> = parse_name_server(&ns6);
    // `NameServer` vazio = DHCP: o DNS do roteador NÃO é guardado como se fosse fixo
    let (ips4, dhcp4) = crate::config::sanitize_original_dns(ips4.clone(), ips4.is_empty());
    let (ips6, dhcp6) = crate::config::sanitize_original_dns6(ips6.clone(), ips6.is_empty());
    Ok(Some(ConnectionBackup {
        id: alias.to_string(),
        was_dhcp_ipv4: dhcp4,
        original_ipv4_dns: ips4,
        original_ipv6_dns: ips6,
        was_dhcp_ipv6: dhcp6,
    }))
}

/// DNS configurados nas interfaces ativas (IPv4 e IPv6).
pub fn configured_dns() -> Vec<std::net::IpAddr> {
    configured_dns_with(&RealRunner)
}

fn configured_dns_with(r: &dyn CommandRunner) -> Vec<std::net::IpAddr> {
    let Ok(ifaces) = list_active_interfaces(r) else {
        return Vec::new();
    };
    let mut out: Vec<std::net::IpAddr> = Vec::new();
    for (_, index) in ifaces {
        let Ok(text) = powershell_output(
            r,
            &format!(
                "(Get-DnsClientServerAddress -InterfaceIndex {index}).ServerAddresses -join ','"
            ),
        ) else {
            continue;
        };
        for tok in text.trim().split(',') {
            if let Ok(ip) = tok.trim().parse::<std::net::IpAddr>() {
                if !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
    }
    out
}

/// `true` se alguma interface ativa ainda usa o DNS do filtro do GuiaLar.
pub fn cloudflare_dns_active() -> bool {
    powershell_output(
        &RealRunner,
        "(Get-DnsClientServerAddress -AddressFamily IPv4).ServerAddresses -join ','",
    )
    .map(|o| {
        o.trim()
            .split(',')
            .filter_map(|s| s.trim().parse::<Ipv4Addr>().ok())
            .any(|ip| crate::config::is_filter_ipv4(&ip))
    })
    .unwrap_or(false)
}

/// Reversão de emergência sem manifesto: DNS automático em todas as interfaces.
pub fn revert_dns_fallback() -> Result<()> {
    if elevate_if_needed("internal-revert")? {
        return Ok(());
    }
    let _lock = crate::sysutil::OpLock::acquire()?;
    reset_all_interfaces(&RealRunner)?;
    crate::policy::revert_all()?;
    flush_dns(&RealRunner);
    remove_manifest_or_warn();
    Ok(())
}

/// DNS automático em todas as interfaces ativas (por índice).
fn reset_all_interfaces(r: &dyn CommandRunner) -> Result<()> {
    for (_, index) in list_active_interfaces(r)? {
        run_powershell(
            r,
            &format!("Set-DnsClientServerAddress -InterfaceIndex {index} -ResetServerAddresses"),
        )?;
    }
    Ok(())
}

fn flush_dns(r: &dyn CommandRunner) {
    let _ = r.run(&Cmd::output("ipconfig", &["/flushdns"]).no_window());
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod runner_tests;

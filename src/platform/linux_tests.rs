//! Testes do backend Linux com `FakeRunner` (ADR-029, camada 2): argv exato, `apply ∘ revert`,
//! idempotência e falha de elevação. Nada aqui toca o DNS, o `/etc` nem a rede da máquina.

use super::*;
use crate::config::FilterMode;
use crate::platform::runner::fake::FakeRunner;
use crate::platform::{next_apply_step, ApplyStep};
use crate::testenv::Sandbox;

const V4: &str = "1.1.1.3 1.0.0.3";
const V6: &str = "2606:4700:4700::1113 2606:4700:4700::1003";
const RESOLV: &str = "/etc/resolv.conf";
const DROPIN_DIR: &str = "/etc/systemd/resolved.conf.d";
const DROPIN: &str = "/etc/systemd/resolved.conf.d/99-guialar.conf";

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// O que `main::apply_flow` faz sobre o backend: decide (`next_apply_step`) e aplica.
fn apply(r: &FakeRunner) -> Result<()> {
    match next_apply_step(|| configured_dns_with(r))? {
        ApplyStep::AlreadyActive => Ok(()),
        ApplyStep::Switch(m) => switch_mode_with(r, m),
        ApplyStep::Fresh => apply_dns_with(r),
    }
}

fn manifest() -> DnsStateManifest {
    DnsStateManifest::load()
        .unwrap()
        .expect("manifesto gravado")
}

fn revert(r: &FakeRunner) -> Result<()> {
    revert_dns_with(r, &manifest())
}

/// Ethernet por DHCP + Wi-Fi com DNS fixo.
fn nm_machine() -> FakeRunner {
    FakeRunner::new()
        .with_nm_conn("uuid-eth", "802-3-ethernet", "", false)
        .with_nm_conn("uuid-wifi", "802-11-wireless", "192.168.0.1 8.8.8.8", true)
}

fn resolved_machine() -> FakeRunner {
    FakeRunner::new().with_path("/run/systemd/resolve")
}

fn resolvconf_machine() -> FakeRunner {
    FakeRunner::new().with_command("resolvconf")
}

fn direct_machine() -> FakeRunner {
    FakeRunner::new().with_file(RESOLV, "nameserver 192.168.1.1\nsearch lan\n")
}

// ---------------------------------------------------------------- detecção

#[test]
fn detecta_o_backend_na_ordem_nm_resolved_resolvconf_direto() {
    let tudo = nm_machine()
        .with_path("/run/systemd/resolve")
        .with_command("resolvconf");
    assert_eq!(
        detect_active_backend_with(&tudo),
        BackendKind::NetworkManager
    );
    // nmcli não responde: cai para o systemd-resolved
    let sem_nm = nm_machine()
        .with_path("/run/systemd/resolve")
        .with_command("resolvconf")
        .deny("nmcli", 1);
    assert_eq!(
        detect_active_backend_with(&sem_nm),
        BackendKind::SystemdResolved
    );
    assert_eq!(
        detect_active_backend_with(&resolved_machine().with_command("resolvconf")),
        BackendKind::SystemdResolved
    );
    assert_eq!(
        detect_active_backend_with(&resolvconf_machine()),
        BackendKind::OpenResolvRunit
    );
    assert_eq!(
        detect_active_backend_with(&direct_machine()),
        BackendKind::DirectResolvConf
    );
    assert_eq!(
        detect_active_backend_with(&FakeRunner::new()),
        BackendKind::DirectResolvConf
    );
}

#[test]
fn dns_configurado_junta_resolv_conf_e_nmcli() {
    let r = FakeRunner::new()
        .with_file(RESOLV, "nameserver 127.0.0.53\noptions edns0\n")
        .with_nm_conn("u", "802-3-ethernet", "", false);
    let ips: Vec<String> = configured_dns_with(&r)
        .iter()
        .map(|i| i.to_string())
        .collect();
    assert_eq!(ips, ["127.0.0.53", "10.0.2.3"]);
}

// ----------------------------------------------------------- NetworkManager

#[test]
fn nm_apply_usa_o_argv_de_producao_na_ordem() {
    let _s = Sandbox::new("nm-apply");
    let r = nm_machine();
    apply_dns_with(&r).unwrap();
    let mut esperado = vec![
        v(&["nmcli", "general", "status"]),
        v(&[
            "nmcli",
            "-t",
            "-f",
            "UUID,TYPE",
            "connection",
            "show",
            "--active",
        ]),
    ];
    for u in ["uuid-eth", "uuid-wifi"] {
        esperado.push(v(&[
            "nmcli",
            "-t",
            "-f",
            "ipv4.dns,ipv4.ignore-auto-dns",
            "connection",
            "show",
            u,
        ]));
        // IPv6: chamada própria, logo depois da do IPv4 (o argv do IPv4 não mudou)
        esperado.push(v(&[
            "nmcli",
            "-t",
            "-f",
            "ipv6.dns,ipv6.ignore-auto-dns",
            "connection",
            "show",
            u,
        ]));
        esperado.push(v(&[
            "nmcli",
            "connection",
            "modify",
            u,
            "ipv4.dns",
            V4,
            "ipv4.ignore-auto-dns",
            "yes",
        ]));
        esperado.push(v(&[
            "nmcli",
            "connection",
            "modify",
            u,
            "ipv6.dns",
            V6,
            "ipv6.ignore-auto-dns",
            "yes",
        ]));
        esperado.push(v(&["nmcli", "connection", "up", "uuid", u]));
    }
    assert_eq!(r.argvs(), esperado);
    // e o manifesto guarda o DNS de antes de cada conexão
    let m = manifest();
    assert_eq!(m.backend, BackendKind::NetworkManager);
    assert_eq!(m.connections.len(), 2);
    assert!(m.connections[0].was_dhcp_ipv4);
    assert!(m.connections[0].original_ipv4_dns.is_empty());
    assert!(!m.connections[1].was_dhcp_ipv4);
    assert_eq!(m.connections[1].original_ipv4_dns.len(), 2);
}

#[test]
fn nm_revert_usa_o_argv_de_producao_na_ordem() {
    let _s = Sandbox::new("nm-revert");
    let r = nm_machine();
    apply_dns_with(&r).unwrap();
    r.clear_calls();
    revert(&r).unwrap();
    let reset6 = |u: &str| {
        v(&[
            "nmcli",
            "connection",
            "modify",
            u,
            "ipv6.dns",
            "",
            "ipv6.ignore-auto-dns",
            "no",
        ])
    };
    let up = |u: &str| v(&["nmcli", "connection", "up", "uuid", u]);
    assert_eq!(
        r.argvs(),
        vec![
            // DHCP: zera o DNS e volta a aceitar o da rede
            v(&[
                "nmcli",
                "connection",
                "modify",
                "uuid-eth",
                "ipv4.dns",
                "",
                "ipv4.ignore-auto-dns",
                "no"
            ]),
            reset6("uuid-eth"),
            up("uuid-eth"),
            // DNS fixo: devolve os IPs de antes
            v(&[
                "nmcli",
                "connection",
                "modify",
                "uuid-wifi",
                "ipv4.dns",
                "192.168.0.1 8.8.8.8"
            ]),
            reset6("uuid-wifi"),
            up("uuid-wifi"),
        ]
    );
}

#[test]
fn nm_apply_depois_revert_devolve_o_estado_original() {
    let s = Sandbox::new("nm-roundtrip");
    let r = nm_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    assert_ne!(r.state(), original);
    assert_eq!(r.nm_prop("uuid-eth", "ipv4.dns"), V4);
    assert_eq!(r.nm_prop("uuid-wifi", "ipv6.dns"), V6);
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
    assert!(!s.manifest_path().exists(), "o revert apaga o manifesto");
    assert_eq!(
        crate::verify::check_status(),
        crate::verify::ProtectionStatus::Unprotected,
        "depois do revert o status volta a desprotegido"
    );
}

#[test]
fn nm_apply_duas_vezes_e_idempotente() {
    let s = Sandbox::new("nm-idem");
    let r = nm_machine();
    let original = r.state();
    apply(&r).unwrap();
    let m1 = manifest();
    let aplicado = r.state();
    r.clear_calls();

    apply(&r).unwrap();
    assert_eq!(r.state(), aplicado, "segundo apply não mexe no DNS");
    assert!(
        r.argvs()
            .iter()
            .all(|a| !a.contains(&"modify".to_string()) && !a.contains(&"up".to_string())),
        "segundo apply não escreve: {:?}",
        r.argvs()
    );
    let m2 = manifest();
    assert_eq!(m1.applied_at_utc, m2.applied_at_utc);
    assert_eq!(
        m1.connections[1].original_ipv4_dns, m2.connections[1].original_ipv4_dns,
        "o DNS de antes não vira o do filtro"
    );

    revert(&r).unwrap();
    assert_eq!(
        r.state(),
        original,
        "revert depois de dois apply = estado original"
    );
    assert!(!s.manifest_path().exists());
}

#[test]
fn nm_trocar_de_modo_mantem_o_dns_original() {
    let _s = Sandbox::new("nm-switch");
    let r = nm_machine();
    let original = r.state();
    apply(&r).unwrap();
    crate::config::set_mode(FilterMode::AdsOnly);
    r.clear_calls();
    apply(&r).unwrap();
    let ads4 = format!(
        "{} {}",
        FilterMode::AdsOnly.ipv4()[0],
        FilterMode::AdsOnly.ipv4()[1]
    );
    let ads6 = format!(
        "{} {}",
        FilterMode::AdsOnly.ipv6()[0],
        FilterMode::AdsOnly.ipv6()[1]
    );
    assert_eq!(
        r.argvs()[1..4],
        [
            v(&[
                "nmcli",
                "connection",
                "modify",
                "uuid-eth",
                "ipv4.dns",
                &ads4,
                "ipv4.ignore-auto-dns",
                "yes"
            ]),
            v(&[
                "nmcli",
                "connection",
                "modify",
                "uuid-eth",
                "ipv6.dns",
                &ads6,
                "ipv6.ignore-auto-dns",
                "yes"
            ]),
            v(&["nmcli", "connection", "up", "uuid", "uuid-eth"]),
        ]
    );
    let m = manifest();
    assert_eq!(m.mode, FilterMode::AdsOnly);
    assert_eq!(
        m.connections[1].original_ipv4_dns.len(),
        2,
        "DNS de antes preservado"
    );
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
}

#[test]
fn nm_dns_trocado_por_fora_e_reaplicado_sem_perder_o_original() {
    let _s = Sandbox::new("nm-foreign");
    let r = nm_machine();
    let original = r.state();
    apply(&r).unwrap();
    // alguém troca o DNS do Wi-Fi por fora
    run_privileged_command(
        &r,
        "nmcli",
        &["connection", "modify", "uuid-wifi", "ipv4.dns", "9.9.9.9"],
    )
    .unwrap();
    apply(&r).unwrap();
    assert_eq!(
        r.nm_prop("uuid-wifi", "ipv4.dns"),
        V4,
        "voltou para o filtro"
    );
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
}

// --------------------------------------------------------- systemd-resolved

#[test]
fn resolved_apply_e_revert_usam_o_argv_de_producao() {
    let s = Sandbox::new("resolved");
    let r = resolved_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    let conf = format!("[Resolve]\nDNS={V4} {V6}\n");
    let calls = r.calls();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].argv, v(&["mkdir", "-p", DROPIN_DIR]));
    assert_eq!(calls[1].argv, v(&["tee", DROPIN]));
    assert_eq!(calls[1].stdin.as_deref(), Some(conf.as_str()));
    assert_eq!(
        calls[2].argv,
        v(&["systemctl", "restart", "systemd-resolved"])
    );
    assert_eq!(manifest().backend, BackendKind::SystemdResolved);

    r.clear_calls();
    revert(&r).unwrap();
    assert_eq!(
        r.argvs(),
        vec![
            v(&["rm", "-f", DROPIN]),
            v(&["systemctl", "restart", "systemd-resolved"])
        ]
    );
    assert_eq!(r.state(), original);
    assert!(!s.manifest_path().exists());
}

#[test]
fn resolved_com_a_pasta_do_dropin_nao_cria_de_novo() {
    let _s = Sandbox::new("resolved-dir");
    let r = resolved_machine().with_dir(DROPIN_DIR);
    apply_dns_with(&r).unwrap();
    assert_eq!(r.argvs()[0], v(&["tee", DROPIN]));
    assert_eq!(r.argvs().len(), 2);
}

#[test]
fn resolved_revert_sem_dropin_nao_faz_nada() {
    let _s = Sandbox::new("resolved-nodrop");
    let r = resolved_machine();
    revert_systemd_resolved(&r).unwrap();
    assert!(r.argvs().is_empty());
}

#[test]
fn resolved_apply_duas_vezes_e_idempotente() {
    let _s = Sandbox::new("resolved-idem");
    let r = resolved_machine();
    let original = r.state();
    apply(&r).unwrap();
    // o resolved mostra o DNS do drop-in pelo resolvectl; aqui o fake não tem resolvectl,
    // então o DNS lido some: o plano é reaplicar, sem sobrescrever o manifesto.
    let m1 = manifest();
    apply(&r).unwrap();
    assert_eq!(m1.applied_at_utc, manifest().applied_at_utc);
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
}

// ------------------------------------------------------- runit / resolvconf

#[test]
fn resolvconf_apply_e_revert_usam_o_argv_de_producao() {
    let _s = Sandbox::new("resolvconf");
    let r = resolvconf_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    let calls = r.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].argv, v(&["resolvconf", "-a", "guialar"]));
    let esperado = "nameserver 1.1.1.3\nnameserver 1.0.0.3\nnameserver 2606:4700:4700::1113\nnameserver 2606:4700:4700::1003\n";
    assert_eq!(calls[0].stdin.as_deref(), Some(esperado));
    assert_ne!(r.state(), original);
    r.clear_calls();
    revert(&r).unwrap();
    assert_eq!(r.argvs(), vec![v(&["resolvconf", "-d", "guialar"])]);
    assert_eq!(r.state(), original);
}

// ------------------------------------------------------------ resolv.conf

#[test]
fn direto_apply_e_revert_usam_o_argv_de_producao() {
    let _s = Sandbox::new("direct");
    let r = direct_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    let calls = r.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].argv, v(&["tee", RESOLV]));
    let novo = "# Gerado pelo GuiaLar Digital - Protecao Familiar\nnameserver 1.1.1.3\nnameserver 1.0.0.3\nnameserver 2606:4700:4700::1113\nnameserver 2606:4700:4700::1003\n";
    assert_eq!(calls[0].stdin.as_deref(), Some(novo));
    assert_eq!(
        manifest().previous_resolv_conf.as_deref(),
        Some("nameserver 192.168.1.1\nsearch lan\n")
    );
    r.clear_calls();
    revert(&r).unwrap();
    let c = r.calls();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].argv, v(&["tee", RESOLV]));
    assert_eq!(
        c[0].stdin.as_deref(),
        Some("nameserver 192.168.1.1\nsearch lan\n")
    );
    assert_eq!(r.state(), original, "resolv.conf volta byte a byte");
}

#[test]
fn direto_apply_duas_vezes_nao_perde_o_resolv_conf_original() {
    let _s = Sandbox::new("direct-idem");
    let r = direct_machine();
    let original = r.state();
    apply(&r).unwrap();
    apply(&r).unwrap();
    assert_eq!(
        manifest().previous_resolv_conf.as_deref(),
        Some("nameserver 192.168.1.1\nsearch lan\n"),
        "o segundo apply não grava o resolv.conf do filtro como 'original'"
    );
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
}

// ------------------------------------------------------- privilégio / UAC

#[test]
fn fora_do_root_o_comando_leva_o_prefixo_de_elevacao() {
    let r = FakeRunner::new().not_root(&["sudo", "-n"]);
    run_privileged_command(&r, "systemctl", &["restart", "systemd-resolved"]).unwrap();
    write_privileged_file(&r, "/etc/x.conf", "dados").unwrap();
    let c = r.calls();
    assert_eq!(
        c[0].argv,
        v(&["sudo", "-n", "systemctl", "restart", "systemd-resolved"])
    );
    assert_eq!(c[1].argv, v(&["sudo", "-n", "tee", "/etc/x.conf"]));
    assert_eq!(c[1].stdin.as_deref(), Some("dados"));
    let p = FakeRunner::new().not_root(&["pkexec"]);
    run_privileged_command(&p, "rm", &["-f", "/etc/x"]).unwrap();
    assert_eq!(p.argvs(), vec![v(&["pkexec", "rm", "-f", "/etc/x"])]);
}

#[test]
fn elevacao_tenta_sudo_sem_senha_e_so_depois_pkexec() {
    let _s = Sandbox::new("elev-ordem");
    let exe = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    // sudo -n funciona: usa sudo e não chama pkexec
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("sudo")
        .with_command("pkexec");
    elevate_self(&r, "internal-revert").unwrap();
    assert_eq!(
        r.argvs(),
        vec![
            v(&["sudo", "-n", "true"]),
            v(&["sudo", "-n", &exe, "internal-revert"])
        ]
    );

    // sudo pede senha: pkexec
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("sudo")
        .with_command("pkexec")
        .deny("sudo", 1);
    elevate_self(&r, "internal-apply --mode ads").unwrap();
    assert_eq!(
        r.argvs(),
        vec![
            v(&["sudo", "-n", "true"]),
            v(&["pkexec", &exe, "internal-apply", "--mode", "ads"])
        ]
    );

    // só pkexec: nem sonda o sudo
    let r = FakeRunner::new().not_root(&[]).with_command("pkexec");
    elevate_self(&r, "internal-revert").unwrap();
    assert_eq!(r.argvs(), vec![v(&["pkexec", &exe, "internal-revert"])]);

    // nenhum dos dois
    let r = FakeRunner::new().not_root(&[]);
    let e = elevate_self(&r, "internal-revert").unwrap_err();
    assert!(format!("{e:#}").contains("elevação indisponível"));
    assert!(r.argvs().is_empty());
}

#[test]
fn elevacao_em_appimage_usa_pasta_privada_com_pkexec_e_sudo() {
    let _s = Sandbox::new("elev-appimage");
    std::env::set_var("APPIMAGE", "/home/u/Gui aLar; rm -rf $HOME.AppImage");
    let ai = "/home/u/Gui aLar; rm -rf $HOME.AppImage";
    // pkexec
    let r = FakeRunner::new().not_root(&[]).with_command("pkexec");
    elevate_self(&r, "internal-revert").unwrap();
    assert_eq!(
        r.argvs(),
        vec![v(&[
            "pkexec",
            "/bin/sh",
            "-c",
            ELEVATED_APPIMAGE_SCRIPT,
            ai,
            "internal-revert"
        ])]
    );
    // sudo: sonda e depois o mesmo comando, com `-n`
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("sudo")
        .with_command("pkexec");
    elevate_self(&r, "internal-apply --mode ads").unwrap();
    assert_eq!(
        r.argvs(),
        vec![
            v(&["sudo", "-n", "true"]),
            v(&[
                "sudo",
                "-n",
                "/bin/sh",
                "-c",
                ELEVATED_APPIMAGE_SCRIPT,
                ai,
                "internal-apply",
                "--mode",
                "ads"
            ])
        ]
    );
    // nada vindo do usuário entra na string do script
    assert!(!ELEVATED_APPIMAGE_SCRIPT.contains("AppImage"));
    assert!(ELEVATED_APPIMAGE_SCRIPT.contains("mktemp -d"));
    assert!(ELEVATED_APPIMAGE_SCRIPT.contains("TMPDIR=\"$d\""));
    assert!(ELEVATED_APPIMAGE_SCRIPT.contains("APPIMAGE_EXTRACT_AND_RUN=1"));
    assert!(ELEVATED_APPIMAGE_SCRIPT.contains("rm -rf -- \"$d\""));
    std::env::remove_var("APPIMAGE");
}

/// Executa o script de verdade com um «AppImage» falso: pasta privada 0700, `TMPDIR` apontando
/// para ela, código de saída preservado e pasta apagada (sucesso ou erro).
#[cfg(unix)]
#[test]
fn script_elevado_cria_pasta_privada_preserva_o_codigo_e_apaga() {
    use std::os::unix::fs::PermissionsExt;
    let s = Sandbox::new("elev-script");
    let fake = s.dir().join("fake appimage.sh");
    // imprime o ambiente que o runtime veria e sai com o código pedido no 1º argumento
    std::fs::write(
        &fake,
        "#!/bin/sh\necho \"TMPDIR=$TMPDIR\"\necho \"EXTRACT=$APPIMAGE_EXTRACT_AND_RUN\"\n\
         echo \"MODE=$(stat -c %a \"$TMPDIR\")\"\necho \"ARGS=$*\"\ntouch \"$TMPDIR/extraido\"\nexit \"$1\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    for code in [0, 7] {
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(ELEVATED_APPIMAGE_SCRIPT)
            .arg(&fake)
            .arg(code.to_string())
            .arg("internal-apply")
            .output()
            .unwrap();
        let so = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(code), "código preservado: {so}");
        let tmpdir = so
            .lines()
            .find_map(|l| l.strip_prefix("TMPDIR="))
            .unwrap()
            .to_string();
        assert!(tmpdir.starts_with("/tmp/guialar-elev."), "{tmpdir}");
        assert!(so.contains("EXTRACT=1"), "{so}");
        assert!(so.contains("MODE=700"), "{so}");
        assert!(so.contains(&format!("ARGS={code} internal-apply")), "{so}");
        assert!(
            !std::path::Path::new(&tmpdir).exists(),
            "a pasta privada some ao final (código {code})"
        );
    }
}

#[test]
fn permissao_negada_no_apply_nao_altera_nada() {
    let s = Sandbox::new("uac-apply");
    let r = nm_machine()
        .not_root(&[])
        .with_command("pkexec")
        .deny("pkexec", 126);
    let original = r.state();
    let e = apply_dns_with(&r).unwrap_err();
    let msg = format!("{e:#}");
    assert!(msg.contains("negada"), "{msg}");
    assert_eq!(r.state(), original, "DNS intacto");
    assert!(!s.manifest_path().exists(), "sem manifesto");
    assert_eq!(
        r.argvs().len(),
        1,
        "só o pedido de elevação: {:?}",
        r.argvs()
    );
    let (curto, detalhe) = crate::ui::theme::humanize_error(&msg);
    assert!(curto.contains("Nada foi alterado"), "{curto}");
    assert_eq!(detalhe, msg);
}

#[test]
fn permissao_negada_no_revert_mantem_manifesto_e_dns_filtrado() {
    let s = Sandbox::new("uac-revert");
    let r = nm_machine().with_command("pkexec");
    apply_dns_with(&r).unwrap();
    let filtrado = r.state();
    let manifesto = std::fs::read_to_string(s.manifest_path()).unwrap();

    r.set_root(false);
    r.add_deny("pkexec", 126);
    let e = revert(&r).unwrap_err();
    let msg = format!("{e:#}");
    assert!(msg.contains("negada"), "{msg}");
    assert_eq!(r.state(), filtrado, "DNS continua no filtro");
    assert_eq!(
        std::fs::read_to_string(s.manifest_path()).unwrap(),
        manifesto
    );
    assert!(crate::ui::theme::humanize_error(&msg)
        .0
        .contains("Nada foi alterado"));

    // tentar de novo e aceitar: agora restaura
    r.clear_deny();
    r.set_root(true);
    revert(&r).unwrap();
    assert!(!s.manifest_path().exists());
}

#[test]
fn comando_que_falha_no_meio_nao_grava_manifesto() {
    let s = Sandbox::new("tee-falha");
    let r = direct_machine().deny("tee", 1);
    let original = r.state();
    let e = apply_dns_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("Falha ao gravar /etc/resolv.conf"));
    assert_eq!(r.state(), original);
    assert!(!s.manifest_path().exists());
}

// ------------------------------------------------------ revert sem manifesto

#[test]
fn sem_manifesto_e_rede_normal_nao_ha_o_que_reverter() {
    let s = Sandbox::new("nomanifest-normal");
    let r = nm_machine();
    let original = r.state();
    assert!(!s.manifest_path().exists());
    assert!(!cloudflare_dns_active_with(&r));
    assert_eq!(r.state(), original);
    assert!(
        r.argvs().iter().all(|a| !a.contains(&"modify".to_string())),
        "só leituras: {:?}",
        r.argvs()
    );
}

#[test]
fn sem_manifesto_com_filtro_ativo_volta_ao_automatico() {
    let s = Sandbox::new("nomanifest-filter");
    let r = nm_machine();
    apply_dns_with(&r).unwrap();
    DnsStateManifest::remove_manifest().unwrap();
    assert!(
        cloudflare_dns_active_with(&r),
        "o filtro ainda está na conexão"
    );
    r.clear_calls();
    revert_dns_fallback_with(&r).unwrap();
    let reset = |u: &str, fam: &str| {
        v(&[
            "nmcli",
            "connection",
            "modify",
            u,
            &format!("{fam}.dns"),
            "",
            &format!("{fam}.ignore-auto-dns"),
            "no",
        ])
    };
    let argv = r.argvs();
    assert_eq!(argv[0], v(&["nmcli", "general", "status"]));
    assert_eq!(
        argv[1],
        v(&[
            "nmcli",
            "-t",
            "-f",
            "UUID,TYPE",
            "connection",
            "show",
            "--active"
        ])
    );
    assert_eq!(argv[2], reset("uuid-eth", "ipv4"));
    assert_eq!(argv[3], reset("uuid-eth", "ipv6"));
    assert_eq!(
        argv[4],
        v(&["nmcli", "connection", "up", "uuid", "uuid-eth"])
    );
    assert_eq!(r.nm_prop("uuid-wifi", "ipv4.dns"), "");
    assert_eq!(r.nm_prop("uuid-wifi", "ipv4.ignore-auto-dns"), "no");
    assert!(!s.manifest_path().exists());
}

#[test]
fn sem_manifesto_o_resolv_conf_so_perde_as_linhas_do_filtro() {
    let _s = Sandbox::new("nomanifest-direct");
    let r = FakeRunner::new().with_file(
        RESOLV,
        "# Gerado pelo GuiaLar Digital - Protecao Familiar\nnameserver 1.1.1.3\nnameserver 192.168.1.1\n",
    );
    revert_dns_fallback_with(&r).unwrap();
    assert_eq!(
        r.read_file(RESOLV).as_deref(),
        Some("nameserver 192.168.1.1\n")
    );
}

#[test]
fn deny_do_programa_vale_mesmo_com_prefixo_de_elevacao() {
    let r = FakeRunner::new().not_root(&["sudo", "-n"]).deny("tee", 1);
    let e = write_privileged_file(&r, "/etc/x", "a").unwrap_err();
    assert!(format!("{e:#}").contains("Falha ao gravar"));
    assert!(r.state().files.is_empty());
}

// ------------------------------------- ticket #24: o revert devolve o DNS fixo (v4 e v6)

/// Roda apply e revert e exige o estado original de volta, em tudo (v4 e v6).
fn ida_e_volta(r: &FakeRunner, nome: &str) {
    let _s = Sandbox::new(nome);
    let original = r.state();
    apply_dns_with(r).unwrap();
    assert_ne!(r.state(), original, "o apply muda algo");
    revert(r).unwrap();
    assert_eq!(r.state(), original, "{nome}: o revert devolve o original");
}

#[test]
fn nm_revert_v4_fixo_com_ignore_auto_dns_no_devolve_o_fixo() {
    // DNS extra fixo SOMADO ao do DHCP (era o defeito: virava vazio)
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "10.0.2.99", false);
    ida_e_volta(&r, "nm24-v4-fixo-no");
    assert_eq!(r.nm_prop("u", "ipv4.dns"), "10.0.2.99");
    assert_eq!(r.nm_prop("u", "ipv4.ignore-auto-dns"), "no");
}

#[test]
fn nm_revert_v4_fixo_com_ignore_auto_dns_yes_devolve_o_fixo() {
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "10.0.2.98 10.0.2.99", true);
    ida_e_volta(&r, "nm24-v4-fixo-yes");
    assert_eq!(r.nm_prop("u", "ipv4.dns"), "10.0.2.98 10.0.2.99");
    assert_eq!(r.nm_prop("u", "ipv4.ignore-auto-dns"), "yes");
}

#[test]
fn nm_revert_v4_sem_fixo_mantem_o_argv_de_antes() {
    let _s = Sandbox::new("nm24-v4-dhcp");
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "", false);
    apply_dns_with(&r).unwrap();
    r.clear_calls();
    revert(&r).unwrap();
    assert_eq!(
        r.argvs()[0],
        v(&[
            "nmcli",
            "connection",
            "modify",
            "u",
            "ipv4.dns",
            "",
            "ipv4.ignore-auto-dns",
            "no"
        ])
    );
}

#[test]
fn nm_revert_v6_fixo_devolve_o_fixo_nos_dois_ignore_auto_dns() {
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", false)
        .with_nm_v6("u", "2001:db8::99 2001:db8::98", false);
    ida_e_volta(&r, "nm24-v6-fixo-no");
    assert_eq!(r.nm_prop("u", "ipv6.dns"), "2001:db8::99 2001:db8::98");
    assert_eq!(r.nm_prop("u", "ipv6.ignore-auto-dns"), "no");

    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", false)
        .with_nm_v6("u", "2001:db8::99", true);
    ida_e_volta(&r, "nm24-v6-fixo-yes");
    assert_eq!(r.nm_prop("u", "ipv6.dns"), "2001:db8::99");
    assert_eq!(r.nm_prop("u", "ipv6.ignore-auto-dns"), "yes");
}

#[test]
fn nm_revert_v4_e_v6_fixos_juntos_em_duas_conexoes() {
    let r = FakeRunner::new()
        .with_nm_conn("a", "802-3-ethernet", "10.0.2.99", false)
        .with_nm_v6("a", "2001:db8::99", false)
        .with_nm_conn("b", "802-11-wireless", "192.168.0.1", true)
        .with_nm_v6("b", "2001:db8::1", true);
    ida_e_volta(&r, "nm24-duas");
}

#[test]
fn nm_dns_v6_so_de_filtro_vira_automatico_no_backup() {
    // apply sobre rede já filtrada: o «original» nunca é o filtro do GuiaLar
    let _s = Sandbox::new("nm24-v6-filtro");
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", false)
        .with_nm_v6("u", V6, true);
    apply_dns_with(&r).unwrap();
    let m = manifest();
    assert!(m.connections[0].original_ipv6_dns.is_empty());
    assert!(m.connections[0].was_dhcp_ipv6);
}

#[test]
fn nm_manifesto_antigo_sem_campos_v6_reverte_como_antes() {
    let s = Sandbox::new("nm24-antigo");
    let r = nm_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    // state.json do formato antigo: sem os campos v6 (ou com `was_dhcp_ipv6=true` e lista vazia)
    let antigo = r#"{"backend":"NetworkManager","applied_at_utc":"1","connections":[
        {"id":"uuid-eth","original_ipv4_dns":[],"was_dhcp_ipv4":true},
        {"id":"uuid-wifi","original_ipv4_dns":["192.168.0.1","8.8.8.8"],"original_ipv6_dns":[],
         "was_dhcp_ipv4":false,"was_dhcp_ipv6":true}],"previous_resolv_conf":null,"mode":"family"}"#;
    std::fs::write(s.manifest_path(), antigo).unwrap();
    r.clear_calls();
    revert(&r).unwrap();
    assert_eq!(r.state(), original);
    let reset6 = |u: &str| {
        v(&[
            "nmcli",
            "connection",
            "modify",
            u,
            "ipv6.dns",
            "",
            "ipv6.ignore-auto-dns",
            "no",
        ])
    };
    let argvs = r.argvs();
    assert!(argvs.contains(&reset6("uuid-eth")), "v6 zerado como hoje");
    assert!(argvs.contains(&reset6("uuid-wifi")), "v6 zerado como hoje");
}

#[test]
fn parse_nm_ipv6_le_ips_e_ignore_auto_dns() {
    let (ips, dhcp) = parse_nm_ipv6("ipv6.dns:\nipv6.ignore-auto-dns:no\n");
    assert!(ips.is_empty() && dhcp);
    let (ips, dhcp) =
        parse_nm_ipv6("ipv6.dns:2001:db8::99,2001:4860:4860::8888\nipv6.ignore-auto-dns:yes\n");
    let ips: Vec<String> = ips.iter().map(|i| i.to_string()).collect();
    assert_eq!(ips, ["2001:db8::99", "2001:4860:4860::8888"]);
    assert!(!dhcp);
    // `-t` com vários campos escapa os dois-pontos
    let (ips, _) = parse_nm_ipv6("ipv6.dns:2001\\:db8\\:\\:99\nipv6.ignore-auto-dns:no\n");
    assert_eq!(ips.len(), 1);
}

#[test]
fn nm_falha_ao_ler_o_v6_nao_impede_o_apply() {
    let _s = Sandbox::new("nm24-v6-falha");
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "", false);
    // nmcli «quebra» só na leitura do v6: o backup cai no padrão antigo (DHCP, sem fixos)
    r.add_fail_argv(&["nmcli", "-t", "-f", "ipv6.dns,ipv6.ignore-auto-dns"]);
    apply_dns_with(&r).unwrap();
    let m = manifest();
    assert!(m.connections[0].was_dhcp_ipv6 && m.connections[0].original_ipv6_dns.is_empty());
}

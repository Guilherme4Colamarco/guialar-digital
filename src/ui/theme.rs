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
    /// Backend de DNS detectado.
    #[cfg_attr(windows, allow(dead_code))]
    pub backend: String,
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
        UiState::Protegido => "Verificação concluída: este computador está protegido.".into(),
        UiState::Desprotegido => {
            "Verificação concluída: este computador não está protegido.".into()
        }
        UiState::Parcial => format!("Verificação concluída: {}", description(state, snapshot)),
        UiState::Trabalhando => "Verificando…".into(),
    }
}

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
pub fn actions_for(state: UiState, missing_blocker: bool) -> Vec<Action> {
    match state {
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
        Action::Activate => "Bloqueia sites perigosos e adultos e os anúncios".into(),
        Action::Browsers => format!("Falta o bloqueador em: {}", missing.join(", ")),
        Action::Check => "Testa a conexão agora".into(),
        Action::Restore => "Desfaz a proteção e remove o bloqueador de anúncios".into(),
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
        UiState::Desprotegido => "Ative a proteção para bloquear sites perigosos e adultos.".into(),
        UiState::Protegido => "Sites perigosos e adultos estão bloqueados neste computador.".into(),
        UiState::Parcial => match snapshot {
            Some(s) if s.manifest_corrupt() => {
                "O registro da proteção está danificado. Clique em \"Desativar proteção\" para limpar e começar de novo."
                    .into()
            }
            Some(s) if s.needs_blocker() => format!(
                "Falta o bloqueador de anúncios em: {}.",
                s.missing_blocker.join(", ")
            ),
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

/// Texto de detalhes técnicos (jargão fica só aqui).
pub fn technical_details(s: &Snapshot, last_error: &str) -> String {
    let mut t = format!("Backend de DNS: {}\n", s.backend);
    // O filtro só é citado quando há proteção gravada.
    if !matches!(s.status, ProtectionStatus::Unprotected) {
        t.push_str("Filtro: Cloudflare for Families (1.1.1.3 / 1.0.0.3)\n");
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
            manifest_path: String::new(),
            backend: String::new(),
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
        assert_eq!(actions_for(UiState::Desprotegido, false), [Activate, Check]);
        assert_eq!(actions_for(UiState::Protegido, false), [Check, Restore]);
        assert_eq!(actions_for(UiState::Parcial, true), [Browsers, Restore]);
        assert_eq!(actions_for(UiState::Parcial, false), [Check, Restore]);
        assert!(actions_for(UiState::Trabalhando, false).is_empty());
        for st in [UiState::Desprotegido, UiState::Protegido, UiState::Parcial] {
            for miss in [false, true] {
                let a = actions_for(st, miss);
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
            actions_for(ui_state(Some(&s), false), s.needs_blocker()),
            [Action::Check, Action::Restore]
        );
        assert!(description(UiState::Parcial, Some(&s)).contains("danificado"));
    }

    #[test]
    fn verificar_sempre_tem_veredito() {
        let un = snap(ProtectionStatus::Unprotected, &[]);
        assert!(verdict(UiState::Desprotegido, Some(&un)).contains("não está protegido"));
        assert!(verdict(UiState::Protegido, None).contains("está protegido"));
    }
}

//! Janela nativa do Linux: GTK 4 + Libadwaita, carregadas em tempo de execução (`dlopen`).
//!
//! Assim o mesmo binário roda em máquinas sem GTK (cai no modo texto) e o build não
//! exige pacotes `-dev`. Requer alvo glibc (`x86_64-unknown-linux-gnu`); em musl a
//! janela fica indisponível e a CLI continua funcionando.

#[cfg(not(target_env = "gnu"))]
pub fn run() -> anyhow::Result<bool> {
    Ok(false)
}

#[cfg(target_env = "gnu")]
pub use imp::run;

#[cfg(target_env = "gnu")]
mod imp {
    use std::ffi::{c_char, c_int, c_uint, c_void, CString};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    use anyhow::Result;

    use crate::ui::theme::{self, Snapshot};

    type P = *mut c_void;

    extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> P;
        fn dlsym(handle: P, name: *const c_char) -> P;
    }

    macro_rules! api {
        ($($name:ident : $ty:ty),* $(,)?) => {
            #[allow(non_snake_case)]
            struct Api { $($name: $ty),* }
            fn load_api() -> Option<Api> {
                let libs = ["libglib-2.0.so.0", "libgobject-2.0.so.0", "libgio-2.0.so.0", "libgtk-4.so.1", "libadwaita-1.so.0"];
                let handles: Vec<P> = libs.iter().map(|l| {
                    let c = CString::new(*l).unwrap();
                    // SAFETY: caminho válido; RTLD_NOW|RTLD_GLOBAL.
                    unsafe { dlopen(c.as_ptr(), 0x2 | 0x100) }
                }).collect();
                if handles.iter().any(|h| h.is_null()) { return None; }
                let sym = |n: &str| -> Option<P> {
                    let c = CString::new(n).unwrap();
                    handles.iter().rev().find_map(|h| {
                        // SAFETY: handle válido de dlopen.
                        let p = unsafe { dlsym(*h, c.as_ptr()) };
                        (!p.is_null()).then_some(p)
                    })
                };
                // SAFETY: cada símbolo é convertido para a assinatura C declarada abaixo.
                unsafe {
                    Some(Api { $($name: std::mem::transmute::<P, $ty>(sym(stringify!($name))?)),* })
                }
            }
        };
    }

    api! {
        g_idle_add: unsafe extern "C" fn(unsafe extern "C" fn(P) -> c_int, P) -> c_uint,
        g_signal_connect_data: unsafe extern "C" fn(P, *const c_char, P, P, P, c_int) -> u64,
        g_application_run: unsafe extern "C" fn(P, c_int, P) -> c_int,
        g_object_unref: unsafe extern "C" fn(P),
        g_free: unsafe extern "C" fn(P),
        g_settings_schema_source_get_default: unsafe extern "C" fn() -> P,
        g_settings_schema_source_lookup: unsafe extern "C" fn(P, *const c_char, c_int) -> P,
        g_settings_schema_unref: unsafe extern "C" fn(P),
        g_settings_new: unsafe extern "C" fn(*const c_char) -> P,
        g_settings_get_string: unsafe extern "C" fn(P, *const c_char) -> P,
        adw_style_manager_get_default: unsafe extern "C" fn() -> P,
        adw_style_manager_set_color_scheme: unsafe extern "C" fn(P, c_int),
        adw_init: unsafe extern "C" fn(),
        adw_application_new: unsafe extern "C" fn(*const c_char, c_int) -> P,
        adw_application_window_new: unsafe extern "C" fn(P) -> P,
        adw_application_window_set_content: unsafe extern "C" fn(P, P),
        adw_header_bar_new: unsafe extern "C" fn() -> P,
        adw_status_page_new: unsafe extern "C" fn() -> P,
        adw_status_page_set_icon_name: unsafe extern "C" fn(P, *const c_char),
        adw_status_page_set_title: unsafe extern "C" fn(P, *const c_char),
        adw_status_page_set_description: unsafe extern "C" fn(P, *const c_char),
        adw_preferences_group_new: unsafe extern "C" fn() -> P,
        adw_preferences_group_add: unsafe extern "C" fn(P, P),
        adw_action_row_new: unsafe extern "C" fn() -> P,
        adw_action_row_set_subtitle: unsafe extern "C" fn(P, *const c_char),
        adw_action_row_add_suffix: unsafe extern "C" fn(P, P),
        adw_preferences_row_set_title: unsafe extern "C" fn(P, *const c_char),
        adw_expander_row_new: unsafe extern "C" fn() -> P,
        adw_expander_row_add_row: unsafe extern "C" fn(P, P),
        adw_clamp_new: unsafe extern "C" fn() -> P,
        adw_clamp_set_child: unsafe extern "C" fn(P, P),
        adw_toast_new: unsafe extern "C" fn(*const c_char) -> P,
        adw_toast_overlay_new: unsafe extern "C" fn() -> P,
        adw_toast_overlay_set_child: unsafe extern "C" fn(P, P),
        adw_toast_overlay_add_toast: unsafe extern "C" fn(P, P),
        gtk_box_new: unsafe extern "C" fn(c_int, c_int) -> P,
        gtk_box_append: unsafe extern "C" fn(P, P),
        gtk_button_new_with_label: unsafe extern "C" fn(*const c_char) -> P,
        gtk_scrolled_window_new: unsafe extern "C" fn() -> P,
        gtk_scrolled_window_set_child: unsafe extern "C" fn(P, P),
        gtk_widget_set_valign: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_vexpand: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_sensitive: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_visible: unsafe extern "C" fn(P, c_int),
        gtk_widget_add_css_class: unsafe extern "C" fn(P, *const c_char),
        gtk_widget_set_margin_top: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_margin_bottom: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_margin_start: unsafe extern "C" fn(P, c_int),
        gtk_widget_set_margin_end: unsafe extern "C" fn(P, c_int),
        gtk_window_set_title: unsafe extern "C" fn(P, *const c_char),
        gtk_window_set_default_size: unsafe extern "C" fn(P, c_int, c_int),
        gtk_window_set_icon_name: unsafe extern "C" fn(P, *const c_char),
        gtk_window_present: unsafe extern "C" fn(P),
        adw_toast_dismiss: unsafe extern "C" fn(P),
        gtk_check_button_new: unsafe extern "C" fn() -> P,
        gtk_check_button_set_group: unsafe extern "C" fn(P, P),
        gtk_check_button_set_active: unsafe extern "C" fn(P, c_int),
        gtk_check_button_get_active: unsafe extern "C" fn(P) -> c_int,
        adw_action_row_add_prefix: unsafe extern "C" fn(P, P),
        adw_action_row_set_activatable_widget: unsafe extern "C" fn(P, P),
        gtk_link_button_new_with_label: unsafe extern "C" fn(*const c_char, *const c_char) -> P,
        g_object_ref: unsafe extern "C" fn(P) -> P,
        gtk_widget_remove_css_class: unsafe extern "C" fn(P, *const c_char),
        gtk_button_set_label: unsafe extern "C" fn(P, *const c_char),
        gtk_progress_bar_new: unsafe extern "C" fn() -> P,
        gtk_progress_bar_pulse: unsafe extern "C" fn(P),
        gtk_widget_get_clipboard: unsafe extern "C" fn(P) -> P,
        gdk_clipboard_set_text: unsafe extern "C" fn(P, *const c_char),
        g_timeout_add: unsafe extern "C" fn(c_uint, unsafe extern "C" fn(P) -> c_int, P) -> c_uint,
        g_simple_action_new: unsafe extern "C" fn(*const c_char, P) -> P,
        g_action_map_add_action: unsafe extern "C" fn(P, P),
        gtk_application_set_accels_for_action: unsafe extern "C" fn(P, *const c_char, *const *const c_char),
    }

    static API: OnceLock<Api> = OnceLock::new();

    fn api() -> &'static Api {
        API.get().expect("API GTK carregada")
    }

    #[derive(Default, Clone, Copy)]
    struct Ui {
        status_page: usize,
        row_dns: usize,
        row_browsers: usize,
        row_manifest: usize,
        toasts: usize,
        progress: usize,
        window: usize,
        row_check: usize,
        buttons: [usize; 4], // ativar, restaurar, navegadores, verificar
        modes_group: usize,
        check_ads: usize,
        check_family: usize,
    }

    static BUSY: AtomicBool = AtomicBool::new(false);
    static ARMED: AtomicBool = AtomicBool::new(false);
    /// `true` enquanto o programa mexe nos rádios (não é escolha do usuário).
    static SYNCING: AtomicBool = AtomicBool::new(false);
    /// Toast "clique de novo…" ainda na tela (com referência própria) para poder dispensá-lo.
    static ARM_TOAST: AtomicUsize = AtomicUsize::new(0);
    /// Verificação pedida pelo usuário: ao terminar, mostra o veredito.
    static CHECKING: AtomicBool = AtomicBool::new(false);
    /// Último veredito de "Verificar": fica na tela (o toast some sozinho).
    static LAST_VERDICT: Mutex<Option<String>> = Mutex::new(None);
    static LAST: Mutex<Option<Snapshot>> = Mutex::new(None);
    static ERR_DETAIL: Mutex<String> = Mutex::new(String::new());

    static UI: Mutex<Option<Ui>> = Mutex::new(None);

    fn ui() -> Ui {
        UI.lock().unwrap().unwrap_or_default()
    }

    // AdwColorScheme
    const ADW_COLOR_SCHEME_DEFAULT: c_int = 0; // segue o portal XDG (GNOME/KDE/COSMIC)
    #[allow(dead_code)]
    const ADW_COLOR_SCHEME_PREFER_DARK: c_int = 3;
    // Força escuro mesmo se o portal responder "claro" (ex.: XFCE com tema GTK -dark, sem portal).
    const ADW_COLOR_SCHEME_FORCE_DARK: c_int = 4;

    const SCHEMA_INTERFACE: &str = "org.gnome.desktop.interface";

    /// GSettings vivo durante o `run` (para os sinais `changed::*`); 0 = schema ausente.
    static GSETTINGS: Mutex<usize> = Mutex::new(0);

    /// `adw_init()` sobrescreve `GtkSettings:gtk-theme-name`; por isso a leitura vem do GSettings,
    /// de `GTK_THEME` e do `settings.ini`, nunca do `GtkSettings` depois do `adw_init`.
    fn gsettings_string(settings: usize, key: &str) -> Option<String> {
        if settings == 0 {
            return None;
        }
        let a = api();
        // SAFETY: `settings` é um GSettings vivo; a string devolvida é nossa e liberada com `g_free`.
        unsafe {
            let p = (a.g_settings_get_string)(settings as P, cs(key).as_ptr());
            if p.is_null() {
                return None;
            }
            let v = std::ffi::CStr::from_ptr(p as *const c_char)
                .to_string_lossy()
                .into_owned();
            (a.g_free)(p);
            Some(v)
        }
    }

    fn settings_ini() -> Option<String> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
            })?;
        std::fs::read_to_string(base.join("gtk-3.0/settings.ini")).ok()
    }

    /// Segue o tema do sistema: escuro detectado => FORCE_DARK (4); senão DEFAULT (0).
    fn sync_system_color_scheme() {
        let a = api();
        let gs = *GSETTINGS.lock().unwrap();
        let env = std::env::var("GTK_THEME").ok();
        let dark = theme::desktop_prefers_dark(
            env.as_deref(),
            gsettings_string(gs, "color-scheme").as_deref(),
            gsettings_string(gs, "gtk-theme").as_deref(),
            settings_ini().as_deref(),
        );
        // SAFETY: chamado na thread principal.
        unsafe {
            let sm = (a.adw_style_manager_get_default)();
            if !sm.is_null() {
                let scheme = if dark {
                    ADW_COLOR_SCHEME_FORCE_DARK
                } else {
                    ADW_COLOR_SCHEME_DEFAULT
                };
                (a.adw_style_manager_set_color_scheme)(sm, scheme);
            }
        }
    }

    unsafe extern "C" fn on_setting_changed(_s: P, _key: P, _d: P) {
        sync_system_color_scheme();
    }

    /// Abre o GSettings da interface só se o schema existir (senão o GLib abortaria o processo).
    fn open_interface_settings() -> usize {
        let a = api();
        // SAFETY: consulta de schema na thread principal; o schema é liberado após o uso.
        unsafe {
            let src = (a.g_settings_schema_source_get_default)();
            if src.is_null() {
                return 0;
            }
            let schema = (a.g_settings_schema_source_lookup)(src, cs(SCHEMA_INTERFACE).as_ptr(), 1);
            if schema.is_null() {
                return 0;
            }
            (a.g_settings_schema_unref)(schema);
            let gs = (a.g_settings_new)(cs(SCHEMA_INTERFACE).as_ptr());
            if gs.is_null() {
                return 0;
            }
            for sig in ["changed::gtk-theme", "changed::color-scheme"] {
                (a.g_signal_connect_data)(
                    gs,
                    cs(sig).as_ptr(),
                    on_setting_changed as P,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    0,
                );
            }
            gs as usize
        }
    }

    fn cs(s: &str) -> CString {
        CString::new(s.replace('\0', " ")).unwrap()
    }

    fn set_row_subtitle(row: usize, text: &str) {
        // SAFETY: widgets vivem enquanto a aplicação roda; chamado só na thread principal.
        unsafe { (api().adw_action_row_set_subtitle)(row as P, cs(text).as_ptr()) };
    }

    fn pick_icon(state: theme::UiState) -> &'static str {
        match state {
            theme::UiState::Protegido => "security-high-symbolic",
            theme::UiState::Parcial => "security-medium-symbolic",
            theme::UiState::Desprotegido => "security-low-symbolic",
            theme::UiState::Trabalhando => "emblem-synchronizing-symbolic",
        }
    }

    /// Redesenha tudo a partir do estado (ADR-020): só a tabela de `theme` decide.
    fn render() {
        use theme::Action;
        let u = ui();
        let a = api();
        let busy = BUSY.load(Ordering::Relaxed);
        let snap = LAST.lock().unwrap().clone();
        let state = theme::ui_state(snap.as_ref(), busy);
        let layout_state = theme::ui_state(snap.as_ref(), false);
        let missing_list = snap
            .as_ref()
            .map(|s| s.missing_blocker.clone())
            .unwrap_or_default();
        let actions = theme::actions_for(
            layout_state,
            snap.as_ref().is_some_and(|s| s.needs_blocker()),
        );
        let (head, desc) = if snap.is_none() {
            (
                "Verificando…",
                "Lendo o estado deste computador.".to_string(),
            )
        } else {
            (
                theme::headline(state),
                theme::description(state, snap.as_ref()),
            )
        };
        let verdict = LAST_VERDICT.lock().unwrap().clone();
        let desc = match &verdict {
            Some(v) if !busy => format!("{desc}\n\nÚltima verificação: {v}"),
            _ => desc,
        };
        // SAFETY: thread principal; ponteiros guardados em UI.
        unsafe {
            (a.adw_status_page_set_icon_name)(u.status_page as P, cs(pick_icon(state)).as_ptr());
            (a.adw_status_page_set_title)(u.status_page as P, cs(head).as_ptr());
            (a.adw_status_page_set_description)(u.status_page as P, cs(&desc).as_ptr());
            for (action, idx) in [
                (Action::Activate, 0),
                (Action::Restore, 1),
                (Action::Browsers, 2),
                (Action::Check, 3),
            ] {
                let b = u.buttons[idx] as P;
                let pos = actions.iter().position(|x| *x == action);
                (a.gtk_widget_set_visible)(b, pos.is_some() as c_int);
                (a.gtk_widget_set_sensitive)(b, (!busy) as c_int);
                let label = if action == Action::Restore && ARMED.load(Ordering::Relaxed) {
                    "Confirmar desativação"
                } else {
                    theme::button_label(action, layout_state)
                };
                (a.gtk_button_set_label)(b, cs(label).as_ptr());
                let accent = cs("suggested-action");
                if pos == Some(0) {
                    (a.gtk_widget_add_css_class)(b, accent.as_ptr());
                } else {
                    (a.gtk_widget_remove_css_class)(b, accent.as_ptr());
                }
            }
            (a.gtk_widget_set_visible)(u.progress as P, busy as c_int);
        }
        // Linhas: título descreve, botão age.
        let protect_row_action = if actions.contains(&Action::Restore) {
            Action::Restore
        } else {
            Action::Activate
        };
        // SAFETY: thread principal.
        unsafe {
            (a.adw_preferences_row_set_title)(
                u.row_dns as P,
                cs(theme::card_title(protect_row_action)).as_ptr(),
            );
            let show_dns =
                actions.contains(&Action::Activate) || actions.contains(&Action::Restore);
            (a.gtk_widget_set_visible)(u.row_dns as P, show_dns as c_int);
            (a.gtk_widget_set_visible)(
                u.row_browsers as P,
                actions.contains(&Action::Browsers) as c_int,
            );
            (a.gtk_widget_set_visible)(u.row_check as P, actions.contains(&Action::Check) as c_int);
            // Seletor de filtro só enquanto desprotegido (escolha antes da ação primária).
            (a.gtk_widget_set_visible)(
                u.modes_group as P,
                theme::show_mode_selector(snap.as_ref(), busy) as c_int,
            );
            (a.gtk_widget_set_sensitive)(u.modes_group as P, (!busy) as c_int);
            // Com proteção gravada, o rádio mostra o modo que está valendo.
            if let Some(m) = snap.as_ref().and_then(|s| s.mode) {
                SYNCING.store(true, Ordering::Relaxed);
                let want_ads = m == crate::config::FilterMode::AdsOnly;
                (a.gtk_check_button_set_active)(u.check_ads as P, want_ads as c_int);
                (a.gtk_check_button_set_active)(u.check_family as P, (!want_ads) as c_int);
                SYNCING.store(false, Ordering::Relaxed);
            }
        }
        set_row_subtitle(
            u.row_check,
            verdict.as_deref().unwrap_or("Testa a conexão agora"),
        );
        set_row_subtitle(
            u.row_dns,
            &theme::card_subtitle(protect_row_action, &missing_list),
        );
        set_row_subtitle(
            u.row_browsers,
            &theme::card_subtitle(Action::Browsers, &missing_list),
        );
        let err = ERR_DETAIL.lock().unwrap().clone();
        let details = snap
            .as_ref()
            .map(|s| theme::technical_details(s, &err))
            .unwrap_or(err);
        set_row_subtitle(u.row_manifest, details.trim_end());
    }

    fn set_busy(busy: bool) {
        BUSY.store(busy, Ordering::Relaxed);
        render();
    }

    unsafe extern "C" fn pulse_tick(_d: P) -> c_int {
        if BUSY.load(Ordering::Relaxed) {
            (api().gtk_progress_bar_pulse)(ui().progress as P);
        }
        1
    }

    unsafe extern "C" fn on_copy_btn(_b: P, _d: P) {
        let u = ui();
        let a = api();
        let text = {
            let err = ERR_DETAIL.lock().unwrap().clone();
            match LAST.lock().unwrap().as_ref() {
                Some(s) => theme::technical_details(s, &err),
                None => err,
            }
        };
        (a.gdk_clipboard_set_text)(
            (a.gtk_widget_get_clipboard)(u.window as P),
            cs(&text).as_ptr(),
        );
        toast("Detalhes copiados.");
    }

    unsafe extern "C" fn on_verify_shortcut(_a: P, _p: P, _d: P) {
        if !BUSY.load(Ordering::Relaxed) {
            CHECKING.store(true, Ordering::Relaxed);
            refresh();
        }
    }

    /// Mostra um toast e devolve o ponteiro (com referência extra: libere com `dismiss_arm_toast`).
    fn toast_ref(msg: &str) -> usize {
        let u = ui();
        let a = api();
        // SAFETY: thread principal. O overlay assume uma referência; guardamos outra.
        unsafe {
            let t = (a.adw_toast_new)(cs(msg).as_ptr());
            (a.g_object_ref)(t);
            (a.adw_toast_overlay_add_toast)(u.toasts as P, t);
            t as usize
        }
    }

    fn toast(msg: &str) {
        let t = toast_ref(msg);
        // SAFETY: devolve a referência extra; o overlay mantém a dele.
        unsafe { (api().g_object_unref)(t as P) };
    }

    /// Dispensa o aviso de "confirmar desativação" (já não vale) e libera a referência.
    fn dismiss_arm_toast() {
        let t = ARM_TOAST.swap(0, Ordering::Relaxed);
        if t != 0 {
            // SAFETY: referência própria guardada em `toast_ref`; dispensar duas vezes é inofensivo.
            unsafe {
                (api().adw_toast_dismiss)(t as P);
                (api().g_object_unref)(t as P);
            }
        }
    }

    unsafe extern "C" fn idle_snapshot(data: P) -> c_int {
        let s = Box::from_raw(data as *mut Snapshot);
        *LAST.lock().unwrap() = Some(*s);
        let checked = CHECKING.swap(false, Ordering::Relaxed);
        let mut verdict = None;
        if checked {
            // Verificação concluída: um erro antigo (ex.: permissão negada) não vale mais.
            ERR_DETAIL.lock().unwrap().clear();
            let snap = LAST.lock().unwrap().clone();
            let state = theme::ui_state(snap.as_ref(), false);
            let v = theme::verdict(state, snap.as_ref());
            *LAST_VERDICT.lock().unwrap() = Some(v.clone());
            verdict = Some(v);
        }
        set_busy(false); // redesenha já com o veredito persistente
        if let Some(v) = verdict {
            toast(&v);
        }
        0
    }

    unsafe extern "C" fn idle_done(data: P) -> c_int {
        let (short, detail) = *Box::from_raw(data as *mut (String, String));
        *ERR_DETAIL.lock().unwrap() = detail;
        toast(&short);
        0
    }

    fn refresh() {
        set_busy(true);
        std::thread::spawn(|| {
            let snap = Box::new(crate::ui::snapshot());
            // SAFETY: g_idle_add é seguro entre threads; o Box é liberado em idle_snapshot.
            unsafe { (api().g_idle_add)(idle_snapshot, Box::into_raw(snap) as P) };
        });
    }

    fn run_action(f: fn() -> Result<String>) {
        *LAST_VERDICT.lock().unwrap() = None; // resultado velho não vale depois de uma ação
        set_busy(true);
        std::thread::spawn(move || {
            let pair = match f() {
                Ok(m) => (m, String::new()),
                Err(e) => theme::humanize_error(&format!("{e:#}")),
            };
            // SAFETY: ver `refresh`.
            unsafe { (api().g_idle_add)(idle_done, Box::into_raw(Box::new(pair)) as P) };
            let snap = Box::new(crate::ui::snapshot());
            unsafe { (api().g_idle_add)(idle_snapshot, Box::into_raw(snap) as P) };
        });
    }

    /// Mudou o rádio: com proteção ligada em outro modo, troca (mantém o DNS original).
    unsafe extern "C" fn on_mode_toggled(b: P, _d: P) {
        if SYNCING.load(Ordering::Relaxed) || BUSY.load(Ordering::Relaxed) {
            return;
        }
        if (api().gtk_check_button_get_active)(b) == 0 {
            return; // evento do botão que ficou inativo
        }
        let chosen = if b as usize == ui().check_ads {
            crate::config::FilterMode::AdsOnly
        } else {
            crate::config::FilterMode::Family
        };
        let active = LAST.lock().unwrap().as_ref().and_then(|s| s.mode);
        if active.is_some_and(|m| m != chosen) {
            crate::config::set_mode(chosen);
            run_action(crate::ui::action_activate);
        }
    }

    unsafe extern "C" fn on_activate_btn(_b: P, _d: P) {
        // Filtro escolhido no seletor (padrão: proteção familiar).
        let ads = (api().gtk_check_button_get_active)(ui().check_ads as P) != 0;
        crate::config::set_mode(if ads {
            crate::config::FilterMode::AdsOnly
        } else {
            crate::config::FilterMode::Family
        });
        run_action(crate::ui::action_activate);
    }
    /// Desativar pede confirmação (E5): o 1º clique arma, o 2º executa; desarma em 5 s.
    unsafe extern "C" fn on_restore_btn(_b: P, _d: P) {
        if ARMED.swap(false, Ordering::Relaxed) {
            dismiss_arm_toast();
            render();
            run_action(crate::ui::action_restore);
        } else {
            ARMED.store(true, Ordering::Relaxed);
            render();
            dismiss_arm_toast();
            ARM_TOAST.store(
                toast_ref("Clique de novo em \"Confirmar desativação\" para desligar a proteção."),
                Ordering::Relaxed,
            );
            (api().g_timeout_add)(5000, disarm, std::ptr::null_mut());
        }
    }
    unsafe extern "C" fn disarm(_d: P) -> c_int {
        ARMED.store(false, Ordering::Relaxed);
        dismiss_arm_toast();
        render();
        0
    }
    unsafe extern "C" fn on_browsers_btn(_b: P, _d: P) {
        run_action(crate::ui::action_browsers);
    }
    unsafe extern "C" fn on_check_btn(_b: P, _d: P) {
        CHECKING.store(true, Ordering::Relaxed);
        refresh();
    }

    fn button(label: &str, class: Option<&str>, cb: unsafe extern "C" fn(P, P)) -> P {
        let a = api();
        // SAFETY: construção de widgets na thread principal.
        unsafe {
            let b = (a.gtk_button_new_with_label)(cs(label).as_ptr());
            (a.gtk_widget_set_valign)(b, 3); // GTK_ALIGN_CENTER
            if let Some(c) = class {
                (a.gtk_widget_add_css_class)(b, cs(c).as_ptr());
            }
            (a.g_signal_connect_data)(
                b,
                cs("clicked").as_ptr(),
                cb as P,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            );
            b
        }
    }

    fn row(title: &str, subtitle: &str, suffixes: &[P]) -> P {
        let a = api();
        // SAFETY: construção de widgets na thread principal.
        unsafe {
            let r = (a.adw_action_row_new)();
            (a.adw_preferences_row_set_title)(r, cs(title).as_ptr());
            (a.adw_action_row_set_subtitle)(r, cs(subtitle).as_ptr());
            for s in suffixes {
                (a.adw_action_row_add_suffix)(r, *s);
            }
            r
        }
    }

    unsafe extern "C" fn on_app_activate(app: P, _d: P) {
        let a = api();
        // Segunda ativação (ex.: abrir de novo pelo menu): só traz a janela existente.
        let existing = UI.lock().unwrap().as_ref().map(|u| u.window);
        if let Some(win) = existing {
            (a.gtk_window_present)(win as P);
            refresh();
            return;
        }
        let win = (a.adw_application_window_new)(app);
        (a.gtk_window_set_title)(win, cs("GuiaLar Digital").as_ptr());
        (a.gtk_window_set_icon_name)(win, cs("br.uniube.pi.guialar").as_ptr());
        (a.gtk_window_set_default_size)(win, 640, 700);

        let root = (a.gtk_box_new)(1, 0);
        (a.gtk_box_append)(root, (a.adw_header_bar_new)());

        let content = (a.gtk_box_new)(1, 18);
        for f in [
            a.gtk_widget_set_margin_top,
            a.gtk_widget_set_margin_bottom,
            a.gtk_widget_set_margin_start,
            a.gtk_widget_set_margin_end,
        ] {
            f(content, 18);
        }

        let status = (a.adw_status_page_new)();
        (a.adw_status_page_set_icon_name)(status, cs("security-low-symbolic").as_ptr());
        (a.adw_status_page_set_title)(status, cs("Verificando…").as_ptr());
        (a.adw_status_page_set_description)(
            status,
            cs("Lendo o estado da proteção deste computador.").as_ptr(),
        );
        // Compacto: com o seletor de filtro a janela fica mais cheia e o título não pode sumir.
        (a.gtk_widget_add_css_class)(status, cs("compact").as_ptr());
        (a.gtk_box_append)(content, status);

        let b_on = button("Proteger este computador", None, on_activate_btn);
        let b_off = button("Desativar proteção", None, on_restore_btn);
        let b_br = button("Proteger navegadores", None, on_browsers_btn);
        let b_chk = button("Verificar", None, on_check_btn);
        let progress = (a.gtk_progress_bar_new)();
        (a.gtk_widget_set_visible)(progress, 0);

        // Seletor de filtro (ADR-022): um grupo de rádio, acima da ação primária.
        let modes_group = (a.adw_preferences_group_new)();
        let radio_family = (a.gtk_check_button_new)();
        let radio_ads = (a.gtk_check_button_new)();
        (a.gtk_check_button_set_group)(radio_ads, radio_family);
        (a.gtk_check_button_set_active)(radio_family, 1);
        for (radio, title, sub) in [
            (
                radio_family,
                crate::config::FilterMode::Family.label(),
                "Bloqueia sites perigosos e adultos, e instala o bloqueador de anúncios",
            ),
            (
                radio_ads,
                crate::config::FilterMode::AdsOnly.label(),
                "Bloqueia anúncios e rastreadores. Não filtra conteúdo adulto e não mexe nos navegadores",
            ),
        ] {
            let r = row(title, sub, &[]);
            (a.adw_action_row_add_prefix)(r, radio);
            (a.adw_action_row_set_activatable_widget)(r, radio);
            (a.adw_preferences_group_add)(modes_group, r);
        }
        for radio in [radio_family, radio_ads] {
            (a.g_signal_connect_data)(
                radio,
                cs("toggled").as_ptr(),
                on_mode_toggled as P,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            );
        }
        (a.gtk_box_append)(content, modes_group);

        let group = (a.adw_preferences_group_new)();
        let row_dns = row("Proteja a família em um passo", "", &[b_off, b_on]);
        let row_br = row("Anúncios ainda podem aparecer", "", &[b_br]);
        let row_chk = row(
            "Conferir se está funcionando",
            "Testa a conexão agora",
            &[b_chk],
        );
        for r in [row_dns, row_br, row_chk] {
            (a.adw_preferences_group_add)(group, r);
        }
        (a.gtk_box_append)(content, progress);
        (a.gtk_box_append)(content, group);

        let details = (a.adw_preferences_group_new)();
        let expander = (a.adw_expander_row_new)();
        (a.adw_preferences_row_set_title)(expander, cs("Detalhes técnicos").as_ptr());
        let b_copy = button("Copiar detalhes", None, on_copy_btn);
        let row_manifest = row("Informações do sistema e do último erro", "—", &[b_copy]);
        (a.adw_expander_row_add_row)(expander, row_manifest);
        (a.adw_preferences_group_add)(details, expander);
        (a.gtk_box_append)(content, details);

        // Navegadores recomendados: só informativo (links oficiais), nada é instalado.
        let recs = (a.adw_preferences_group_new)();
        let rec_exp = (a.adw_expander_row_new)();
        (a.adw_preferences_row_set_title)(rec_exp, cs("Navegadores recomendados").as_ptr());
        for r in &theme::RECOMMENDED_BROWSERS {
            let link =
                (a.gtk_link_button_new_with_label)(cs(r.url).as_ptr(), cs("Abrir site").as_ptr());
            (a.gtk_widget_set_valign)(link, 3);
            let rw = row(&format!("{} — {}", r.name, r.profile), r.note, &[link]);
            (a.adw_expander_row_add_row)(rec_exp, rw);
        }
        (a.adw_preferences_group_add)(recs, rec_exp);
        (a.gtk_box_append)(content, recs);

        let clamp = (a.adw_clamp_new)();
        (a.adw_clamp_set_child)(clamp, content);
        let scroll = (a.gtk_scrolled_window_new)();
        (a.gtk_widget_set_vexpand)(scroll, 1);
        (a.gtk_scrolled_window_set_child)(scroll, clamp);
        (a.gtk_box_append)(root, scroll);

        let toasts = (a.adw_toast_overlay_new)();
        (a.adw_toast_overlay_set_child)(toasts, root);
        (a.adw_application_window_set_content)(win, toasts);

        *UI.lock().unwrap() = Some(Ui {
            status_page: status as usize,
            row_dns: row_dns as usize,
            row_browsers: row_br as usize,
            row_manifest: row_manifest as usize,
            toasts: toasts as usize,
            progress: progress as usize,
            window: win as usize,
            row_check: row_chk as usize,
            buttons: [b_on as usize, b_off as usize, b_br as usize, b_chk as usize],
            modes_group: modes_group as usize,
            check_ads: radio_ads as usize,
            check_family: radio_family as usize,
        });
        // Ctrl+R = verificar.
        let act = (a.g_simple_action_new)(cs("verify").as_ptr(), std::ptr::null_mut());
        (a.g_signal_connect_data)(
            act,
            cs("activate").as_ptr(),
            on_verify_shortcut as P,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        );
        (a.g_action_map_add_action)(app, act);
        let accels = [cs("<Control>r"), cs("F5")];
        let ptrs: [*const c_char; 3] = [accels[0].as_ptr(), accels[1].as_ptr(), std::ptr::null()];
        (a.gtk_application_set_accels_for_action)(app, cs("app.verify").as_ptr(), ptrs.as_ptr());
        (a.g_timeout_add)(120, pulse_tick, std::ptr::null_mut());
        render();
        (a.gtk_window_present)(win);
        refresh();
    }

    pub fn run() -> Result<bool> {
        let Some(api) = load_api() else {
            eprintln!("ℹ️  GTK4/Libadwaita não encontrados neste sistema: usando o modo texto.");
            return Ok(false);
        };
        let _ = API.set(api);
        let a = api_ref();
        // SAFETY: bibliotecas carregadas; todas as chamadas ocorrem na thread principal.
        unsafe {
            (a.adw_init)();
            *GSETTINGS.lock().unwrap() = open_interface_settings();
            sync_system_color_scheme();
            let app = (a.adw_application_new)(cs("br.uniube.pi.guialar").as_ptr(), 0);
            (a.g_signal_connect_data)(
                app,
                cs("activate").as_ptr(),
                on_app_activate as P,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            );
            let code = (a.g_application_run)(app, 0, std::ptr::null_mut());
            (a.g_object_unref)(app);
            if code != 0 {
                anyhow::bail!("a janela terminou com código {code}");
            }
        }
        Ok(true)
    }

    fn api_ref() -> &'static Api {
        api()
    }
}

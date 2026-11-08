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
    use std::sync::{Mutex, OnceLock};

    use anyhow::Result;

    use crate::ui::theme::{self, Snapshot};
    use crate::verify::ProtectionStatus;

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
        adw_preferences_group_set_title: unsafe extern "C" fn(P, *const c_char),
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
        gtk_spinner_new: unsafe extern "C" fn() -> P,
        gtk_spinner_set_spinning: unsafe extern "C" fn(P, c_int),
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
        row_log: usize,
        toasts: usize,
        spinner: usize,
        buttons: [usize; 4], // ativar, restaurar, navegadores, verificar
    }

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

    fn pick_icon(status: &ProtectionStatus) -> &'static str {
        match status {
            ProtectionStatus::Protected => "security-high-symbolic",
            ProtectionStatus::PartiallyConfigured(_) => "security-medium-symbolic",
            ProtectionStatus::Unprotected => "security-low-symbolic",
        }
    }

    fn apply_snapshot(s: &Snapshot) {
        let u = ui();
        let a = api();
        // SAFETY: thread principal; ponteiros guardados em UI.
        unsafe {
            (a.adw_status_page_set_icon_name)(
                u.status_page as P,
                cs(pick_icon(&s.status)).as_ptr(),
            );
            (a.adw_status_page_set_title)(
                u.status_page as P,
                cs(theme::headline(&s.status)).as_ptr(),
            );
            (a.adw_status_page_set_description)(
                u.status_page as P,
                cs(&theme::description(&s.status)).as_ptr(),
            );
        }
        set_row_subtitle(u.row_dns, &format!("Backend detectado: {}", s.backend));
        set_row_subtitle(u.row_browsers, &theme::browsers_summary(&s.browsers));
        set_row_subtitle(u.row_manifest, &s.manifest_path);
    }

    fn set_busy(busy: bool) {
        let u = ui();
        let a = api();
        // SAFETY: thread principal.
        unsafe {
            for b in u.buttons {
                (a.gtk_widget_set_sensitive)(b as P, (!busy) as c_int);
            }
            (a.gtk_widget_set_visible)(u.spinner as P, busy as c_int);
            (a.gtk_spinner_set_spinning)(u.spinner as P, busy as c_int);
        }
    }

    fn toast(msg: &str) {
        let u = ui();
        let a = api();
        // SAFETY: thread principal.
        unsafe {
            (a.adw_toast_overlay_add_toast)(u.toasts as P, (a.adw_toast_new)(cs(msg).as_ptr()));
        }
        set_row_subtitle(u.row_log, msg);
    }

    unsafe extern "C" fn idle_snapshot(data: P) -> c_int {
        let s = Box::from_raw(data as *mut Snapshot);
        apply_snapshot(&s);
        set_busy(false);
        0
    }

    unsafe extern "C" fn idle_done(data: P) -> c_int {
        let m = Box::from_raw(data as *mut String);
        toast(&m);
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
        set_busy(true);
        std::thread::spawn(move || {
            let msg = match f() {
                Ok(m) => m,
                Err(e) => format!("Não foi possível concluir: {e:#}"),
            };
            // SAFETY: ver `refresh`.
            unsafe { (api().g_idle_add)(idle_done, Box::into_raw(Box::new(msg)) as P) };
            let snap = Box::new(crate::ui::snapshot());
            unsafe { (api().g_idle_add)(idle_snapshot, Box::into_raw(snap) as P) };
        });
    }

    unsafe extern "C" fn on_activate_btn(_b: P, _d: P) {
        run_action(crate::ui::action_activate);
    }
    unsafe extern "C" fn on_restore_btn(_b: P, _d: P) {
        run_action(crate::ui::action_restore);
    }
    unsafe extern "C" fn on_browsers_btn(_b: P, _d: P) {
        run_action(crate::ui::action_browsers);
    }
    unsafe extern "C" fn on_check_btn(_b: P, _d: P) {
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
        (a.gtk_box_append)(content, status);

        let b_on = button("Ativar", Some("suggested-action"), on_activate_btn);
        let b_off = button("Restaurar", None, on_restore_btn);
        let b_br = button("Proteger Navegadores", None, on_browsers_btn);
        let b_chk = button("Verificar Agora", None, on_check_btn);
        let spinner = (a.gtk_spinner_new)();
        (a.gtk_widget_set_valign)(spinner, 3);
        (a.gtk_widget_set_visible)(spinner, 0);

        let group = (a.adw_preferences_group_new)();
        (a.adw_preferences_group_set_title)(group, cs("Ações de Proteção").as_ptr());
        let row_dns = row(
            "Filtro de Rede (DNS Seguro)",
            "Cloudflare for Families (1.1.1.3 / 1.0.0.3)",
            &[b_off, b_on],
        );
        let row_br = row(
            "Bloqueador de Anúncios (uBlock Origin)",
            "Procurando navegadores…",
            &[b_br],
        );
        let row_chk = row(
            "Diagnóstico da Rede",
            "Testa se o filtro familiar está respondendo",
            &[spinner, b_chk],
        );
        for r in [row_dns, row_br, row_chk] {
            (a.adw_preferences_group_add)(group, r);
        }
        (a.gtk_box_append)(content, group);

        let details = (a.adw_preferences_group_new)();
        let expander = (a.adw_expander_row_new)();
        (a.adw_preferences_row_set_title)(
            expander,
            cs("Detalhes Técnicos e Registro de Eventos").as_ptr(),
        );
        let row_manifest = row("Manifesto de reversão", "—", &[]);
        let row_log = row("Última ação", "Nenhuma ainda", &[]);
        (a.adw_expander_row_add_row)(expander, row_manifest);
        (a.adw_expander_row_add_row)(expander, row_log);
        (a.adw_preferences_group_add)(details, expander);
        (a.gtk_box_append)(content, details);

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
            row_log: row_log as usize,
            toasts: toasts as usize,
            spinner: spinner as usize,
            buttons: [b_on as usize, b_off as usize, b_br as usize, b_chk as usize],
        });
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

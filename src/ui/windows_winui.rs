//! Janela nativa do Windows 11: Win32 + DWM (Mica, cantos arredondados, tema escuro/claro)
//! com cartões no estilo "Configurações" (Fluent). Sem dependências externas.
//!
//! Observação: não usamos o WinUI 3 (Windows App SDK) porque ele exigiria runtime/MSIX
//! e quebraria o executável único portátil (ADR-018, seção 2.3).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;

use anyhow::{bail, Result};

use super::theme::{self, Palette, Snapshot};
use crate::win32::*;

const ID_ACTIVATE: usize = 101;
const ID_BROWSERS: usize = 102;
const ID_CHECK: usize = 103;
const ID_RESTORE: usize = 104;
const ID_DETAILS: usize = 106;
const ID_COPY: usize = 107;
const TIMER_PROGRESS: usize = 1;
const WM_APP_DONE: u32 = 0x8001;
const WM_APP_SNAPSHOT: u32 = 0x8002;
const BCM_SETSHIELD: u32 = 0x160C;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static DARK: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<AppState> = Mutex::new(AppState {
    snapshot: None,
    message: String::new(),
    error_detail: String::new(),
    busy: false,
    details_open: false,
    tick: 0,
});

struct AppState {
    snapshot: Option<Snapshot>,
    message: String,
    /// Cadeia técnica do último erro (vai só para "Detalhes").
    error_detail: String,
    busy: bool,
    details_open: bool,
    tick: u32,
}

struct Done {
    message: String,
    detail: String,
}

struct Fonts {
    title: HGDIOBJ,
    subtitle: HGDIOBJ,
    body: HGDIOBJ,
    icons: HGDIOBJ,
}

static FONTS: Mutex<Option<[usize; 4]>> = Mutex::new(None);
static BUTTONS: Mutex<[usize; 4]> = Mutex::new([0; 4]);
/// [alternar detalhes, copiar detalhes]
static EXTRA: Mutex<[usize; 2]> = Mutex::new([0; 2]);

fn palette() -> Palette {
    if DARK.load(Ordering::Relaxed) {
        theme::DARK
    } else {
        theme::LIGHT
    }
}

fn read_light_theme(value: &str) -> Option<u32> {
    let sub = w(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let val = w(value);
    let mut data: u32 = 1;
    let mut size: u32 = 4;
    // SAFETY: ponteiros válidos para a duração da chamada.
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            sub.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut data as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    (rc == 0).then_some(data)
}

/// Escuro quando `AppsUseLightTheme` (ou, na falta, `SystemUsesLightTheme`) vale 0.
fn system_prefers_dark() -> bool {
    read_light_theme("AppsUseLightTheme").or_else(|| read_light_theme("SystemUsesLightTheme"))
        == Some(0)
}

fn dp(px: i32, dpi: u32) -> i32 {
    px * dpi as i32 / 96
}

fn make_font(pt: i32, weight: i32, face: &str, dpi: u32) -> HGDIOBJ {
    // SAFETY: `face` termina em zero; a fonte é liberada em `free_fonts`.
    unsafe {
        CreateFontW(
            -(pt * dpi as i32 / 72),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            5, // CLEARTYPE_QUALITY
            0,
            w(face).as_ptr(),
        )
    }
}

fn load_fonts(dpi: u32) -> Fonts {
    let ui = "Segoe UI Variable Display";
    let f = Fonts {
        title: make_font(22, 600, ui, dpi),
        subtitle: make_font(14, 600, ui, dpi),
        body: make_font(11, 400, ui, dpi),
        icons: make_font(22, 400, "Segoe MDL2 Assets", dpi),
    };
    free_fonts();
    *FONTS.lock().unwrap() = Some([
        f.title as usize,
        f.subtitle as usize,
        f.body as usize,
        f.icons as usize,
    ]);
    f
}

fn free_fonts() {
    if let Some(old) = FONTS.lock().unwrap().take() {
        for h in old {
            // SAFETY: fontes criadas por `make_font`.
            unsafe { DeleteObject(h as HGDIOBJ) };
        }
    }
}

fn current_fonts() -> Option<Fonts> {
    FONTS.lock().unwrap().map(|f| Fonts {
        title: f[0] as HGDIOBJ,
        subtitle: f[1] as HGDIOBJ,
        body: f[2] as HGDIOBJ,
        icons: f[3] as HGDIOBJ,
    })
}

struct Layout {
    margin: i32,
    header: RECT,
    hero: RECT,
    cards: Vec<RECT>,
    status: RECT,
    details_toggle: RECT,
    details_panel: RECT,
}

fn layout(client_w: i32, dpi: u32, n_cards: usize) -> Layout {
    let m = dp(24, dpi);
    let wd = client_w - 2 * m;
    let mut y = dp(16, dpi);
    let header = RECT {
        left: m,
        top: y,
        right: m + wd,
        bottom: y + dp(64, dpi),
    };
    y = header.bottom + dp(12, dpi);
    let hero = RECT {
        left: m,
        top: y,
        right: m + wd,
        bottom: y + dp(112, dpi),
    };
    y = hero.bottom + dp(14, dpi);
    let mut cards = Vec::with_capacity(n_cards);
    for _ in 0..n_cards {
        let c = RECT {
            left: m,
            top: y,
            right: m + wd,
            bottom: y + dp(76, dpi),
        };
        y = c.bottom + dp(8, dpi);
        cards.push(c);
    }
    let status = RECT {
        left: m,
        top: y + dp(4, dpi),
        right: m + wd,
        bottom: y + dp(48, dpi),
    };
    let details_toggle = RECT {
        left: m,
        top: status.bottom + dp(4, dpi),
        right: m + dp(200, dpi),
        bottom: status.bottom + dp(4 + 36, dpi),
    };
    let details_panel = RECT {
        left: m,
        top: details_toggle.bottom + dp(6, dpi),
        right: m + wd,
        bottom: details_toggle.bottom + dp(6 + 150, dpi),
    };
    Layout {
        margin: m,
        header,
        hero,
        cards,
        status,
        details_toggle,
        details_panel,
    }
}

fn button_rect(card: &RECT, dpi: u32, wide: i32) -> RECT {
    let bw = dp(wide, dpi);
    let bh = dp(38, dpi);
    let cy = (card.top + card.bottom) / 2;
    RECT {
        left: card.right - dp(16, dpi) - bw,
        top: cy - bh / 2,
        right: card.right - dp(16, dpi),
        bottom: cy + bh / 2,
    }
}

fn apply_theme(hwnd: HWND) {
    let dark = DARK.load(Ordering::Relaxed) as i32;
    let backdrop: i32 = 2; // DWMSBT_MAINWINDOW = Mica
    let corner: i32 = 2; // DWMWCP_ROUND
                         // SAFETY: ponteiros para inteiros locais; falhas (Windows antigo) são ignoradas.
    unsafe {
        let p = palette();
        let caption = rgb(p.background);
        let text = rgb(p.text);
        allow_dark_mode_for_window(hwnd, dark == 1);
        DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 19, &dark as *const i32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 38, &backdrop as *const i32 as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 33, &corner as *const i32 as *const c_void, 4);
        let theme = w(if dark == 1 {
            "DarkMode_Explorer"
        } else {
            "Explorer"
        });
        let mut all: Vec<usize> = BUTTONS.lock().unwrap().to_vec();
        all.extend(EXTRA.lock().unwrap().iter());
        for b in all.iter().filter(|b| **b != 0) {
            allow_dark_mode_for_window(*b as HWND, dark == 1);
            SetWindowTheme(*b as HWND, theme.as_ptr(), std::ptr::null());
            InvalidateRect(*b as HWND, std::ptr::null(), 1);
        }
    }
}

fn create_button(parent: HWND, id: usize, text: &str, primary: bool) -> HWND {
    let style = 0x5000_0000 | if primary { 0x1 } else { 0 }; // WS_CHILD|WS_VISIBLE|WS_TABSTOP, BS_DEFPUSHBUTTON
                                                             // SAFETY: classe "BUTTON" do sistema; strings terminadas em zero.
    unsafe {
        CreateWindowExW(
            0,
            w("BUTTON").as_ptr(),
            w(text).as_ptr(),
            style,
            0,
            0,
            10,
            10,
            parent,
            id as *mut c_void,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        )
    }
}

fn actions_of(snapshot: &Option<Snapshot>) -> Vec<theme::Action> {
    let state = theme::ui_state(snapshot.as_ref(), false);
    let missing = snapshot
        .as_ref()
        .is_some_and(|s| !s.missing_blocker.is_empty());
    theme::actions_for(state, missing)
}

fn current_actions() -> Vec<theme::Action> {
    actions_of(&STATE.lock().unwrap().snapshot)
}

fn action_id(a: theme::Action) -> usize {
    match a {
        theme::Action::Activate => ID_ACTIVATE,
        theme::Action::Browsers => ID_BROWSERS,
        theme::Action::Check => ID_CHECK,
        theme::Action::Restore => ID_RESTORE,
    }
}

fn place_buttons(hwnd: HWND) {
    let mut rc = RECT::default();
    // SAFETY: `hwnd` é a janela principal.
    let dpi = unsafe {
        GetClientRect(hwnd, &mut rc);
        GetDpiForWindow(hwnd).max(96)
    };
    let (actions, ui_st, open) = {
        let st = STATE.lock().unwrap();
        let a = actions_of(&st.snapshot);
        (
            a,
            theme::ui_state(st.snapshot.as_ref(), false),
            st.details_open,
        )
    };
    let l = layout(rc.right, dpi, actions.len());
    let b = *BUTTONS.lock().unwrap();
    let all = [
        theme::Action::Activate,
        theme::Action::Browsers,
        theme::Action::Check,
        theme::Action::Restore,
    ];
    for (i, h) in b.iter().enumerate() {
        let pos = actions.iter().position(|a| *a as usize == i);
        // SAFETY: handles criados em WM_CREATE.
        unsafe {
            match pos {
                Some(k) => {
                    let r = button_rect(&l.cards[k], dpi, 190);
                    MoveWindow(
                        *h as HWND,
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        1,
                    );
                    SetWindowTextW(*h as HWND, w(theme::button_label(all[i], ui_st)).as_ptr());
                    // Primeira ação = primária: botão padrão (Enter).
                    SendMessageW(*h as HWND, 0x00F4, (k == 0) as usize, 1); // BM_SETSTYLE
                    ShowWindow(*h as HWND, 5); // SW_SHOW
                }
                None => {
                    ShowWindow(*h as HWND, SW_HIDE);
                }
            }
        }
    }
    let e = *EXTRA.lock().unwrap();
    let t = l.details_toggle;
    let panel = l.details_panel;
    let cw = dp(170, dpi);
    let ch = dp(34, dpi);
    // SAFETY: handles criados em WM_CREATE.
    unsafe {
        MoveWindow(
            e[0] as HWND,
            t.left,
            t.top,
            t.right - t.left,
            t.bottom - t.top,
            1,
        );
        SetWindowTextW(
            e[0] as HWND,
            w(if open {
                "Detalhes técnicos  \u{25B4}"
            } else {
                "Detalhes técnicos  \u{25BE}"
            })
            .as_ptr(),
        );
        MoveWindow(
            e[1] as HWND,
            panel.right - dp(12, dpi) - cw,
            panel.bottom - dp(10, dpi) - ch,
            cw,
            ch,
            1,
        );
        ShowWindow(e[1] as HWND, if open { 5 } else { SW_HIDE });
        if let Some(first) = actions.first() {
            let idx = *first as usize;
            let _ = SetFocus(b[idx] as HWND);
        }
    }
}

fn set_busy(hwnd: HWND, busy: bool) {
    STATE.lock().unwrap().busy = busy;
    for b in BUTTONS.lock().unwrap().iter() {
        // SAFETY: handles de botões filhos.
        unsafe { EnableWindow(*b as HWND, (!busy) as i32) };
    }
    // SAFETY: janela principal.
    unsafe {
        EnableWindow(EXTRA.lock().unwrap()[1] as HWND, (!busy) as i32);
        if busy {
            SetTimer(hwnd, TIMER_PROGRESS, 40, std::ptr::null());
        } else {
            KillTimer(hwnd, TIMER_PROGRESS);
        }
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

fn spawn_snapshot() {
    std::thread::spawn(|| {
        let snap = Box::new(super::snapshot());
        post(WM_APP_SNAPSHOT, Box::into_raw(snap) as isize);
    });
}

fn spawn_action(hwnd: HWND, f: fn() -> Result<String>) {
    if STATE.lock().unwrap().busy {
        return;
    }
    STATE.lock().unwrap().message =
        "Se aparecer a janela de permissão do Windows, escolha Sim.".into();
    set_busy(hwnd, true);
    std::thread::spawn(move || {
        let done = match f() {
            Ok(m) => Done {
                message: m,
                detail: String::new(),
            },
            Err(e) => {
                let (short, detail) = theme::humanize_error(&format!("{e:#}"));
                Done {
                    message: short,
                    detail,
                }
            }
        };
        post(WM_APP_DONE, Box::into_raw(Box::new(done)) as isize);
    });
}

fn post(msg: u32, lparam: isize) {
    let h = MAIN_HWND.load(Ordering::Relaxed) as HWND;
    // SAFETY: a janela principal vive até o fim do loop; se falhar o Box vaza (inofensivo).
    unsafe { PostMessageW(h, msg, 0, lparam) };
}

fn text(dc: HDC, font: HGDIOBJ, color: u32, s: &str, r: &mut RECT, fmt: u32) {
    // SAFETY: `dc` válido durante WM_PAINT.
    unsafe {
        SelectObject(dc, font);
        SetTextColor(dc, rgb(color));
        let wide: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(dc, wide.as_ptr(), wide.len() as i32, r, fmt);
    }
}

const DT_LEFT_WORD: u32 = 0x10 | 0x800; // DT_WORDBREAK | DT_NOPREFIX
const DT_ONE_LINE: u32 = 0x20 | 0x8000 | 0x800 | 0x4; // SINGLELINE | END_ELLIPSIS | NOPREFIX | VCENTER

fn card_frame(dc: HDC, r: &RECT, p: &Palette, dpi: u32) {
    // SAFETY: objetos GDI criados e liberados aqui.
    unsafe {
        let brush = CreateSolidBrush(rgb(p.card));
        let pen = CreatePen(0, 1, rgb(p.border));
        let (ob, op) = (SelectObject(dc, brush), SelectObject(dc, pen));
        let rad = dp(8, dpi);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, rad, rad);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

fn paint(hwnd: HWND) {
    // SAFETY: ciclo BeginPaint/EndPaint com buffer duplo.
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let wdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc);
        let dpi = GetDpiForWindow(hwnd).max(96);
        let dc = CreateCompatibleDC(wdc);
        let bmp = CreateCompatibleBitmap(wdc, rc.right.max(1), rc.bottom.max(1));
        let old = SelectObject(dc, bmp);
        draw(dc, &rc, dpi);
        BitBlt(wdc, 0, 0, rc.right, rc.bottom, dc, 0, 0, 0x00CC0020);
        SelectObject(dc, old);
        DeleteObject(bmp);
        DeleteDC(dc);
        EndPaint(hwnd, &ps);
    }
}

fn draw(dc: HDC, rc: &RECT, dpi: u32) {
    let p = palette();
    let Some(f) = current_fonts() else { return };
    let (snapshot, message, busy, tick, open, err) = {
        let s = STATE.lock().unwrap();
        (
            s.snapshot.clone(),
            s.message.clone(),
            s.busy,
            s.tick,
            s.details_open,
            s.error_detail.clone(),
        )
    };
    let actions = actions_of(&snapshot);
    let l = layout(rc.right, dpi, actions.len());
    // SAFETY: GDI no DC em memória desta pintura.
    unsafe {
        let bg = CreateSolidBrush(rgb(p.background));
        FillRect(dc, rc, bg);
        DeleteObject(bg);
        SetBkMode(dc, 1); // TRANSPARENT
    }

    // Cabeçalho: ícone + título + subtítulo.
    let icon_px = dp(48, dpi);
    // SAFETY: ícone do recurso 1 (se existir).
    unsafe {
        let icon = LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            int_resource(1),
            1,
            icon_px,
            icon_px,
            0,
        );
        if !icon.is_null() {
            DrawIconEx(
                dc,
                l.header.left,
                l.header.top + dp(4, dpi),
                icon,
                icon_px,
                icon_px,
                0,
                std::ptr::null_mut(),
                3,
            );
        }
    }
    let tx = l.header.left + icon_px + dp(14, dpi);
    let mut r = RECT {
        left: tx,
        top: l.header.top,
        right: l.header.right - dp(120, dpi),
        bottom: l.header.top + dp(34, dpi),
    };
    text(dc, f.title, p.text, "GuiaLar Digital", &mut r, DT_ONE_LINE);
    let mut r = RECT {
        left: tx,
        top: l.header.top + dp(34, dpi),
        right: l.header.right - dp(120, dpi),
        bottom: l.header.bottom,
    };
    text(
        dc,
        f.body,
        p.text_secondary,
        "Segurança familiar e proteção de navegação",
        &mut r,
        DT_ONE_LINE,
    );

    // Hero de status.
    let ui_st = theme::ui_state(snapshot.as_ref(), busy);
    card_frame(dc, &l.hero, &p, dpi);
    let color = theme::state_color(&p, ui_st);
    let glyph = match ui_st {
        theme::UiState::Protegido => "\u{E73E}",
        theme::UiState::Parcial => "\u{E7BA}",
        theme::UiState::Desprotegido => "\u{E783}",
        theme::UiState::Trabalhando => "\u{E895}",
    };
    let (head, desc) = if snapshot.is_none() && !busy {
        (
            "Verificando…".to_string(),
            "Lendo o estado deste computador.".to_string(),
        )
    } else {
        (
            theme::headline(ui_st).to_string(),
            theme::description(ui_st, snapshot.as_ref()),
        )
    };
    let d = dp(60, dpi);
    let (bx, by) = (
        l.hero.left + dp(20, dpi),
        l.hero.top + (l.hero.bottom - l.hero.top - d) / 2,
    );
    // SAFETY: GDI.
    unsafe {
        let br = CreateSolidBrush(rgb(color));
        let pen = CreatePen(5, 0, 0); // PS_NULL
        let (ob, op) = (SelectObject(dc, br), SelectObject(dc, pen));
        Ellipse(dc, bx, by, bx + d, by + d);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(br);
        DeleteObject(pen);
    }
    let mut gr = RECT {
        left: bx,
        top: by,
        right: bx + d,
        bottom: by + d,
    };
    text(dc, f.icons, 0xFFFFFF, glyph, &mut gr, 0x1 | 0x20 | 0x4); // CENTER|SINGLELINE|VCENTER
    let hx = bx + d + dp(18, dpi);
    let mut r = RECT {
        left: hx,
        top: l.hero.top + dp(18, dpi),
        right: l.hero.right - dp(16, dpi),
        bottom: l.hero.top + dp(48, dpi),
    };
    text(dc, f.subtitle, color, &head, &mut r, DT_ONE_LINE);
    let mut r = RECT {
        left: hx,
        top: l.hero.top + dp(50, dpi),
        right: l.hero.right - dp(16, dpi),
        bottom: l.hero.bottom - dp(8, dpi),
    };
    text(dc, f.body, p.text_secondary, &desc, &mut r, DT_LEFT_WORD);

    // Cartões: só as ações do estado (primária primeiro).
    let missing = snapshot
        .as_ref()
        .map(|s| s.missing_blocker.clone())
        .unwrap_or_default();
    for (c, a) in l.cards.iter().zip(actions.iter()) {
        card_frame(dc, c, &p, dpi);
        let right = c.right - dp(16 + 190 + 16, dpi);
        let mut r = RECT {
            left: c.left + dp(20, dpi),
            top: c.top + dp(12, dpi),
            right,
            bottom: c.top + dp(40, dpi),
        };
        text(
            dc,
            f.subtitle,
            p.text,
            theme::card_title(*a),
            &mut r,
            DT_ONE_LINE,
        );
        let mut r = RECT {
            left: c.left + dp(20, dpi),
            top: c.top + dp(42, dpi),
            right,
            bottom: c.bottom - dp(8, dpi),
        };
        text(
            dc,
            f.body,
            p.text_secondary,
            &theme::card_subtitle(*a, &missing),
            &mut r,
            DT_ONE_LINE,
        );
    }

    // Progresso indeterminado (N1) + mensagem de resultado.
    if busy {
        let track = RECT {
            left: l.status.left,
            top: l.status.top,
            right: l.status.right,
            bottom: l.status.top + dp(4, dpi),
        };
        let seg = (track.right - track.left) / 4;
        let span = (track.right - track.left) + seg;
        let x = track.left - seg + (tick as i32 * dp(10, dpi)) % span;
        let seg_rc = RECT {
            left: x.max(track.left),
            top: track.top,
            right: (x + seg).min(track.right),
            bottom: track.bottom,
        };
        // SAFETY: GDI.
        unsafe {
            let bt = CreateSolidBrush(rgb(p.border));
            FillRect(dc, &track, bt);
            DeleteObject(bt);
            if seg_rc.right > seg_rc.left {
                let bs = CreateSolidBrush(rgb(if DARK.load(Ordering::Relaxed) {
                    0x60CDFF
                } else {
                    0x0067C0
                }));
                FillRect(dc, &seg_rc, bs);
                DeleteObject(bs);
            }
        }
    }
    let mut r = RECT {
        left: l.status.left,
        top: l.status.top + dp(10, dpi),
        right: l.status.right,
        bottom: l.status.bottom,
    };
    text(dc, f.body, p.text_secondary, &message, &mut r, DT_LEFT_WORD);

    // Detalhes técnicos (recolhível): jargão só aqui.
    if open {
        card_frame(dc, &l.details_panel, &p, dpi);
        let body = snapshot
            .as_ref()
            .map(|s| theme::technical_details(s, &err))
            .unwrap_or_else(|| err.clone());
        let mut r = RECT {
            left: l.details_panel.left + dp(16, dpi),
            top: l.details_panel.top + dp(10, dpi),
            right: l.details_panel.right - dp(16, dpi),
            bottom: l.details_panel.bottom - dp(54, dpi),
        };
        text(dc, f.body, p.text_secondary, &body, &mut r, DT_LEFT_WORD);
    }
    let _ = l.margin;
}

/// Copia os detalhes técnicos para a área de transferência (via `clip.exe`, UTF-16).
fn copy_details() {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    let text = {
        let st = STATE.lock().unwrap();
        match &st.snapshot {
            Some(s) => theme::technical_details(s, &st.error_detail),
            None => st.error_detail.clone(),
        }
    };
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    if let Ok(mut child) = std::process::Command::new("clip")
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&bytes);
        }
        let _ = child.wait();
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        0x0001 => {
            // WM_CREATE
            MAIN_HWND.store(hwnd as isize, Ordering::Relaxed);
            let dpi = GetDpiForWindow(hwnd).max(96);
            load_fonts(dpi);
            let mut b = BUTTONS.lock().unwrap();
            b[0] = create_button(hwnd, ID_ACTIVATE, "Proteger este computador", true) as usize;
            b[1] = create_button(hwnd, ID_BROWSERS, "Proteger navegadores", false) as usize;
            b[2] = create_button(hwnd, ID_CHECK, "Verificar", false) as usize;
            b[3] = create_button(hwnd, ID_RESTORE, "Desativar proteção", false) as usize;
            SendMessageW(b[0] as HWND, BCM_SETSHIELD, 0, 1); // escudo UAC
            SendMessageW(b[3] as HWND, BCM_SETSHIELD, 0, 1);
            drop(b);
            let mut e = EXTRA.lock().unwrap();
            e[0] = create_button(hwnd, ID_DETAILS, "Detalhes técnicos", false) as usize;
            e[1] = create_button(hwnd, ID_COPY, "Copiar detalhes", false) as usize;
            drop(e);
            apply_theme(hwnd);
            place_buttons(hwnd);
            spawn_snapshot();
            0
        }
        0x0005 => {
            // WM_SIZE
            if lp != 0 && !BUTTONS.lock().unwrap().iter().all(|b| *b == 0) {
                place_buttons(hwnd);
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        0x000F => {
            paint(hwnd);
            0
        }
        0x0014 => 1, // WM_ERASEBKGND: pintura própria (sem piscar)
        0x02E0 => {
            // WM_DPICHANGED
            let r = *(lp as *const RECT);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                0x4 | 0x10,
            );
            load_fonts(GetDpiForWindow(hwnd).max(96));
            place_buttons(hwnd);
            0
        }
        0x001A | 0x031A => {
            // WM_SETTINGCHANGE / WM_THEMECHANGED: segue o tema do Windows ao vivo.
            DARK.store(system_prefers_dark(), Ordering::Relaxed);
            apply_theme(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        0x0400 => {
            // DM_GETDEFID: Enter aciona a ação primária.
            match current_actions().first() {
                Some(a) => ((0x534Bu32 << 16) | action_id(*a) as u32) as isize,
                None => 0,
            }
        }
        0x0113 => {
            // WM_TIMER: anima a barra de progresso.
            let mut st = STATE.lock().unwrap();
            st.tick = st.tick.wrapping_add(1);
            drop(st);
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        0x0111 => {
            // WM_COMMAND
            let mut id = wp & 0xFFFF;
            if id == 1 {
                // IDOK: Enter fora de um botão = ação primária.
                id = current_actions()
                    .first()
                    .map(|a| action_id(*a))
                    .unwrap_or(0);
            }
            match id {
                ID_ACTIVATE => spawn_action(hwnd, super::action_activate),
                ID_BROWSERS => spawn_action(hwnd, super::action_browsers),
                ID_RESTORE => spawn_action(hwnd, super::action_restore),
                ID_CHECK => {
                    if !STATE.lock().unwrap().busy {
                        STATE.lock().unwrap().message = "Verificando…".into();
                        set_busy(hwnd, true);
                        spawn_snapshot();
                    }
                }
                ID_DETAILS => {
                    {
                        let mut st = STATE.lock().unwrap();
                        st.details_open = !st.details_open;
                    }
                    place_buttons(hwnd);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                ID_COPY => {
                    copy_details();
                    STATE.lock().unwrap().message = "Detalhes copiados.".into();
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                2 => {
                    PostMessageW(hwnd, 0x0010, 0, 0); // IDCANCEL (Esc) -> WM_CLOSE
                }
                _ => {}
            }
            0
        }
        WM_APP_DONE => {
            let d = Box::from_raw(lp as *mut Done);
            {
                let mut st = STATE.lock().unwrap();
                st.message = d.message;
                st.details_open = !d.detail.is_empty();
                st.error_detail = d.detail;
            }
            set_busy(hwnd, false);
            spawn_snapshot();
            0
        }
        WM_APP_SNAPSHOT => {
            let s = Box::from_raw(lp as *mut Snapshot);
            {
                let mut st = STATE.lock().unwrap();
                st.snapshot = Some(*s);
                if st.message.starts_with("Verificando") {
                    st.message = "Verificação concluída.".into();
                }
            }
            set_busy(hwnd, false);
            place_buttons(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        0x0002 => {
            free_fonts();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub fn run() -> Result<()> {
    enable_dark_mode_for_process();
    DARK.store(system_prefers_dark(), Ordering::Relaxed);
    let class = w("GuiaLarDigitalWindow");
    // SAFETY: registro e criação de janela Win32 na thread principal.
    unsafe {
        let inst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0x3, // CS_HREDRAW | CS_VREDRAW
            lpfnWndProc: wndproc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: inst,
            hIcon: LoadImageW(inst, int_resource(1), 1, 32, 32, 0x8000), // LR_SHARED
            hCursor: LoadCursorW(std::ptr::null_mut(), int_resource(32512)),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: LoadImageW(inst, int_resource(1), 1, 16, 16, 0x8000),
        };
        if RegisterClassExW(&wc) == 0 {
            bail!("não foi possível registrar a janela");
        }
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            w("GuiaLar Digital").as_ptr(),
            0x00CF_0000, // WS_OVERLAPPEDWINDOW
            0x8000_0000u32 as i32,
            0x8000_0000u32 as i32,
            700,
            700,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            inst,
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            bail!("não foi possível criar a janela");
        }
        let dpi = GetDpiForWindow(hwnd).max(96);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            dp(700, dpi),
            dp(700, dpi),
            0x2 | 0x4 | 0x10,
        ); // NOMOVE|NOZORDER|NOACTIVATE
        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            // Ctrl+R = verificar.
            if msg.message == 0x0100 && msg.wParam == 0x52 && GetKeyState(0x11) < 0 {
                PostMessageW(hwnd, 0x0111, ID_CHECK, 0);
                continue;
            }
            // Tab/Shift+Tab/Enter/Esc como em um diálogo.
            if IsDialogMessageW(hwnd, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

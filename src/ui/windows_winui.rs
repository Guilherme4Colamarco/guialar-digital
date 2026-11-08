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
use crate::verify::ProtectionStatus;
use crate::win32::*;

const ID_ACTIVATE: usize = 101;
const ID_BROWSERS: usize = 102;
const ID_CHECK: usize = 103;
const ID_RESTORE: usize = 104;
const WM_APP_DONE: u32 = 0x8001;
const WM_APP_SNAPSHOT: u32 = 0x8002;
const BCM_SETSHIELD: u32 = 0x160C;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static DARK: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<UiState> = Mutex::new(UiState {
    snapshot: None,
    message: String::new(),
    busy: false,
});

struct UiState {
    snapshot: Option<Snapshot>,
    message: String,
    busy: bool,
}

struct Fonts {
    title: HGDIOBJ,
    subtitle: HGDIOBJ,
    body: HGDIOBJ,
    icons: HGDIOBJ,
}

static FONTS: Mutex<Option<[usize; 4]>> = Mutex::new(None);
static BUTTONS: Mutex<[usize; 4]> = Mutex::new([0; 4]);

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
    cards: [RECT; 4],
    status: RECT,
}

fn layout(client_w: i32, dpi: u32) -> Layout {
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
    let mut cards = [RECT::default(); 4];
    for c in cards.iter_mut() {
        *c = RECT {
            left: m,
            top: y,
            right: m + wd,
            bottom: y + dp(68, dpi),
        };
        y = c.bottom + dp(8, dpi);
    }
    let status = RECT {
        left: m,
        top: y + dp(4, dpi),
        right: m + wd,
        bottom: y + dp(52, dpi),
    };
    Layout {
        margin: m,
        header,
        hero,
        cards,
        status,
    }
}

fn button_rect(card: &RECT, dpi: u32, wide: i32) -> RECT {
    let bw = dp(wide, dpi);
    let bh = dp(34, dpi);
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
        for b in BUTTONS.lock().unwrap().iter().filter(|b| **b != 0) {
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

fn place_buttons(hwnd: HWND) {
    let mut rc = RECT::default();
    // SAFETY: `hwnd` é a janela principal.
    let dpi = unsafe {
        GetClientRect(hwnd, &mut rc);
        GetDpiForWindow(hwnd).max(96)
    };
    let l = layout(rc.right, dpi);
    let b = *BUTTONS.lock().unwrap();
    let spec: [(usize, RECT, i32); 4] = [
        (b[0], button_rect(&l.cards[0], dpi, 170), 0),
        (b[1], button_rect(&l.cards[1], dpi, 170), 0),
        (b[2], button_rect(&l.cards[2], dpi, 170), 0),
        (b[3], button_rect(&l.cards[3], dpi, 170), 0),
    ];
    for (h, r, _) in spec {
        // SAFETY: handles criados em WM_CREATE.
        unsafe {
            MoveWindow(
                h as HWND,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                1,
            )
        };
    }
}

fn set_busy(hwnd: HWND, busy: bool) {
    STATE.lock().unwrap().busy = busy;
    for (i, b) in BUTTONS.lock().unwrap().iter().enumerate() {
        let _ = i;
        // SAFETY: handles de botões filhos.
        unsafe { EnableWindow(*b as HWND, (!busy) as i32) };
    }
    // SAFETY: janela principal.
    unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
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
        "Trabalhando… confirme a janela de permissão, se aparecer.".into();
    set_busy(hwnd, true);
    std::thread::spawn(move || {
        let msg = match f() {
            Ok(m) => m,
            Err(e) => format!("Não foi possível concluir: {e:#}"),
        };
        post(WM_APP_DONE, Box::into_raw(Box::new(msg)) as isize);
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
    let l = layout(rc.right, dpi);
    let (snapshot, message, busy) = {
        let s = STATE.lock().unwrap();
        (s.snapshot.clone(), s.message.clone(), s.busy)
    };
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
    card_frame(dc, &l.hero, &p, dpi);
    let (head, desc, color, glyph) = match &snapshot {
        Some(s) => (
            theme::headline(&s.status).to_string(),
            theme::description(&s.status),
            theme::status_color(&p, &s.status),
            match s.status {
                ProtectionStatus::Protected => "\u{E73E}",
                ProtectionStatus::PartiallyConfigured(_) => "\u{E7BA}",
                ProtectionStatus::Unprotected => "\u{E783}",
            },
        ),
        None => (
            "Verificando…".into(),
            "Lendo o estado da proteção deste computador.".into(),
            p.text_secondary,
            "\u{E895}",
        ),
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

    // Cartões de ação.
    let browsers = snapshot
        .as_ref()
        .map(|s| theme::browsers_summary(&s.browsers))
        .unwrap_or_else(|| "Procurando navegadores…".into());
    let cards: [(&str, String); 4] = [
        (
            "Ativar Proteção Completa",
            "DNS seguro (Cloudflare Family) + bloqueador de anúncios uBlock".into(),
        ),
        ("Proteger Navegadores", format!("Detectados: {browsers}")),
        (
            "Verificar Agora",
            "Testa se a rede está usando o filtro familiar".into(),
        ),
        (
            "Restaurar Padrão da Rede",
            "Desfaz o DNS e remove o bloqueador instalado pelo GuiaLar".into(),
        ),
    ];
    for (c, (title, sub)) in l.cards.iter().zip(cards.iter()) {
        card_frame(dc, c, &p, dpi);
        let right = c.right - dp(16 + 170 + 16, dpi);
        let mut r = RECT {
            left: c.left + dp(20, dpi),
            top: c.top + dp(10, dpi),
            right,
            bottom: c.top + dp(36, dpi),
        };
        text(dc, f.subtitle, p.text, title, &mut r, DT_ONE_LINE);
        let mut r = RECT {
            left: c.left + dp(20, dpi),
            top: c.top + dp(37, dpi),
            right,
            bottom: c.bottom - dp(6, dpi),
        };
        text(dc, f.body, p.text_secondary, sub, &mut r, DT_ONE_LINE);
    }

    // Barra de status.
    let msg = if message.is_empty() && !busy {
        "Pronto.".to_string()
    } else {
        message
    };
    let mut r = l.status;
    text(dc, f.body, p.text_secondary, &msg, &mut r, DT_LEFT_WORD);
    let _ = l.margin;
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    match msg {
        0x0001 => {
            // WM_CREATE
            MAIN_HWND.store(hwnd as isize, Ordering::Relaxed);
            let dpi = GetDpiForWindow(hwnd).max(96);
            load_fonts(dpi);
            let mut b = BUTTONS.lock().unwrap();
            b[0] = create_button(hwnd, ID_ACTIVATE, "Ativar Proteção", true) as usize;
            b[1] = create_button(hwnd, ID_BROWSERS, "Proteger Navegadores", false) as usize;
            b[2] = create_button(hwnd, ID_CHECK, "Verificar Agora", false) as usize;
            b[3] = create_button(hwnd, ID_RESTORE, "Restaurar Padrão", false) as usize;
            SendMessageW(b[0] as HWND, BCM_SETSHIELD, 0, 1); // escudo UAC
            SendMessageW(b[3] as HWND, BCM_SETSHIELD, 0, 1);
            drop(b);
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
        0x0111 => {
            // WM_COMMAND
            match wp & 0xFFFF {
                ID_ACTIVATE => spawn_action(hwnd, super::action_activate),
                ID_BROWSERS => spawn_action(hwnd, super::action_browsers),
                ID_RESTORE => spawn_action(hwnd, super::action_restore),
                ID_CHECK => {
                    STATE.lock().unwrap().message = "Verificando…".into();
                    STATE.lock().unwrap().snapshot = None;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    spawn_snapshot();
                }
                _ => {}
            }
            0
        }
        WM_APP_DONE => {
            let m = Box::from_raw(lp as *mut String);
            STATE.lock().unwrap().message = *m;
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
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

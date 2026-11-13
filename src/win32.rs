//! Declarações mínimas da API Win32 (FFI manual: sem crates externos no binário).
#![allow(
    non_snake_case,
    clippy::upper_case_acronyms,
    clippy::too_many_arguments
)]

use std::ffi::c_void;

pub type HWND = *mut c_void;
pub type HANDLE = *mut c_void;
pub type HDC = *mut c_void;
pub type HGDIOBJ = *mut c_void;
pub type WNDPROC = unsafe extern "system" fn(HWND, u32, usize, isize) -> isize;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: usize,
    pub lParam: isize,
    pub time: u32,
    pub pt: POINT,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: WNDPROC,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HANDLE,
    pub hIcon: HANDLE,
    pub hCursor: HANDLE,
    pub hbrBackground: HANDLE,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HANDLE,
}

#[repr(C)]
pub struct PAINTSTRUCT {
    pub hdc: HDC,
    pub fErase: i32,
    pub rcPaint: RECT,
    pub fRestore: i32,
    pub fIncUpdate: i32,
    pub rgbReserved: [u8; 32],
}

#[repr(C)]
pub struct SHELLEXECUTEINFOW {
    pub cbSize: u32,
    pub fMask: u32,
    pub hwnd: HWND,
    pub lpVerb: *const u16,
    pub lpFile: *const u16,
    pub lpParameters: *const u16,
    pub lpDirectory: *const u16,
    pub nShow: i32,
    pub hInstApp: HANDLE,
    pub lpIDList: *mut c_void,
    pub lpClass: *const u16,
    pub hkeyClass: HANDLE,
    pub dwHotKey: u32,
    pub hIconOrMonitor: HANDLE,
    pub hProcess: HANDLE,
}

pub const SEE_MASK_NOCLOSEPROCESS: u32 = 0x40;
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

#[link(name = "kernel32")]
extern "system" {
    pub fn AttachConsole(pid: u32) -> i32;
    pub fn GetStdHandle(which: u32) -> HANDLE;
    pub fn SetStdHandle(which: u32, h: HANDLE) -> i32;
    pub fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        sec: *mut c_void,
        disp: u32,
        flags: u32,
        template: HANDLE,
    ) -> HANDLE;
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    pub fn GetExitCodeProcess(h: HANDLE, code: *mut u32) -> i32;
    pub fn CloseHandle(h: HANDLE) -> i32;
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
    pub fn LoadLibraryW(name: *const u16) -> HANDLE;
    pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *mut c_void;
}

/// `HKEY_CURRENT_USER`: `(HKEY)(ULONG_PTR)(LONG)0x80000001` — precisa de sign-extension em 64 bits.
pub const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
pub const RRF_RT_REG_DWORD: u32 = 0x0000_0010;

/// Ponteiros (como `usize`; 0 = indisponível) dos ordinais 135, 136 e 133 do `uxtheme.dll`.
static UXTHEME: std::sync::OnceLock<[usize; 3]> = std::sync::OnceLock::new();

/// Funções não documentadas do `uxtheme.dll`, resolvidas uma só vez (modo escuro dos controles Win32).
fn uxtheme_fns() -> [usize; 3] {
    *UXTHEME.get_or_init(|| {
        // SAFETY: `#[link(name = "uxtheme")]` já carrega a DLL; o ordinal vai no lugar do ponteiro de nome.
        unsafe {
            let mut m = GetModuleHandleW(w("uxtheme.dll").as_ptr());
            if m.is_null() {
                m = LoadLibraryW(w("uxtheme.dll").as_ptr());
            }
            if m.is_null() {
                return [0; 3];
            }
            [135usize, 136, 133].map(|o| GetProcAddress(m, o as *const u8) as usize)
        }
    })
}

/// Ordinal 135 `SetPreferredAppMode(AllowDark)` + ordinal 136 `FlushMenuThemes()`.
pub fn enable_dark_mode_for_process() {
    let f = uxtheme_fns();
    // SAFETY: assinaturas conhecidas dos ordinais; ponteiro nulo (Windows antigo) é ignorado.
    unsafe {
        if f[0] != 0 {
            let g: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(f[0]);
            g(1);
        }
        if f[1] != 0 {
            let g: unsafe extern "system" fn() = std::mem::transmute(f[1]);
            g();
        }
    }
}

/// Ordinal 133 `AllowDarkModeForWindow(hwnd, allow)`.
pub fn allow_dark_mode_for_window(hwnd: HWND, allow: bool) {
    let f = uxtheme_fns();
    // SAFETY: assinatura conhecida do ordinal; ponteiro nulo é ignorado.
    unsafe {
        if f[2] != 0 {
            let g: unsafe extern "system" fn(HWND, i32) -> i32 = std::mem::transmute(f[2]);
            g(hwnd, allow as i32);
        }
    }
}

#[link(name = "shell32")]
extern "system" {
    pub fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> i32;
    pub fn IsUserAnAdmin() -> i32;
    pub fn ShellExecuteW(
        h: HWND,
        verb: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> isize;
}

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(c: *const WNDCLASSEXW) -> u16;
    pub fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: HWND,
        menu: HANDLE,
        inst: HANDLE,
        param: *mut c_void,
    ) -> HWND;
    pub fn ShowWindow(h: HWND, cmd: i32) -> i32;
    pub fn UpdateWindow(h: HWND) -> i32;
    pub fn GetMessageW(m: *mut MSG, h: HWND, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(m: *const MSG) -> i32;
    pub fn DispatchMessageW(m: *const MSG) -> isize;
    pub fn PostQuitMessage(code: i32);
    pub fn DefWindowProcW(h: HWND, m: u32, w: usize, l: isize) -> isize;
    pub fn PostMessageW(h: HWND, m: u32, w: usize, l: isize) -> i32;
    pub fn SendMessageW(h: HWND, m: u32, w: usize, l: isize) -> isize;
    pub fn InvalidateRect(h: HWND, r: *const RECT, erase: i32) -> i32;
    pub fn BeginPaint(h: HWND, ps: *mut PAINTSTRUCT) -> HDC;
    pub fn EndPaint(h: HWND, ps: *const PAINTSTRUCT) -> i32;
    pub fn GetClientRect(h: HWND, r: *mut RECT) -> i32;
    pub fn MoveWindow(h: HWND, x: i32, y: i32, w: i32, hh: i32, repaint: i32) -> i32;
    pub fn SetWindowPos(h: HWND, after: HWND, x: i32, y: i32, w: i32, hh: i32, flags: u32) -> i32;
    pub fn EnableWindow(h: HWND, enable: i32) -> i32;
    pub fn LoadCursorW(inst: HANDLE, name: *const u16) -> HANDLE;
    pub fn LoadImageW(
        inst: HANDLE,
        name: *const u16,
        kind: u32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> HANDLE;
    pub fn DrawIconEx(
        hdc: HDC,
        x: i32,
        y: i32,
        icon: HANDLE,
        cx: i32,
        cy: i32,
        step: u32,
        br: HANDLE,
        flags: u32,
    ) -> i32;
    pub fn GetDpiForWindow(h: HWND) -> u32;
    pub fn SetForegroundWindow(h: HWND) -> i32;
    pub fn FindWindowW(class: *const u16, title: *const u16) -> HWND;
    pub fn DestroyWindow(h: HWND) -> i32;
    pub fn MessageBoxW(h: HWND, text: *const u16, caption: *const u16, kind: u32) -> i32;
    pub fn SetTimer(h: HWND, id: usize, ms: u32, f: *const c_void) -> usize;
    pub fn KillTimer(h: HWND, id: usize) -> i32;
    pub fn IsDialogMessageW(h: HWND, m: *const MSG) -> i32;
    pub fn GetKeyState(vk: i32) -> i16;
    pub fn SetFocus(h: HWND) -> HWND;
    pub fn SetWindowTextW(h: HWND, text: *const u16) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateSolidBrush(color: u32) -> HGDIOBJ;
    pub fn CreatePen(style: i32, width: i32, color: u32) -> HGDIOBJ;
    pub fn DeleteObject(o: HGDIOBJ) -> i32;
    pub fn SelectObject(dc: HDC, o: HGDIOBJ) -> HGDIOBJ;
    pub fn SetBkMode(dc: HDC, mode: i32) -> i32;
    pub fn SetTextColor(dc: HDC, color: u32) -> u32;
    pub fn SetBkColor(dc: HDC, color: u32) -> u32;
    pub fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out: u32,
        clip: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> HGDIOBJ;
    pub fn DrawTextW(dc: HDC, text: *const u16, n: i32, r: *mut RECT, fmt: u32) -> i32;
    pub fn RoundRect(dc: HDC, l: i32, t: i32, r: i32, b: i32, rw: i32, rh: i32) -> i32;
    pub fn Ellipse(dc: HDC, l: i32, t: i32, r: i32, b: i32) -> i32;
    pub fn FillRect(dc: HDC, r: *const RECT, br: HGDIOBJ) -> i32;
    pub fn CreateCompatibleDC(dc: HDC) -> HDC;
    pub fn CreateCompatibleBitmap(dc: HDC, w: i32, h: i32) -> HGDIOBJ;
    pub fn DeleteDC(dc: HDC) -> i32;
    pub fn BitBlt(
        dst: HDC,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        src: HDC,
        sx: i32,
        sy: i32,
        rop: u32,
    ) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmSetWindowAttribute(h: HWND, attr: u32, data: *const c_void, size: u32) -> i32;
}

#[link(name = "uxtheme")]
extern "system" {
    pub fn SetWindowTheme(h: HWND, app: *const u16, list: *const u16) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegGetValueW(
        key: isize,
        sub: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

/// `&str` -> UTF-16 terminado em zero.
pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `MAKEINTRESOURCE`: id numérico como ponteiro de nome de recurso.
pub fn int_resource(id: usize) -> *const u16 {
    id as *const u16
}

/// `RGB` `0xRRGGBB` -> `COLORREF` (`0x00BBGGRR`).
pub fn rgb(c: u32) -> u32 {
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

/// Janela principal: dona do diálogo de UAC (nunca fica atrás nem órfã).
static OWNER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

pub fn set_owner(h: HWND) {
    OWNER.store(h as isize, std::sync::atomic::Ordering::Relaxed);
}

/// Limite de espera da operação elevada (o worker nunca congela para sempre).
const ELEVATED_TIMEOUT_MS: u32 = 120_000;

/// Executa `exe args` elevado (UAC), sem janela, esperando o fim. Devolve o exit code.
pub fn run_elevated_hidden(exe: &str, args: &str) -> Result<u32, String> {
    let verb = w("runas");
    let file = w(exe);
    let params = w(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: OWNER.load(std::sync::atomic::Ordering::Relaxed) as HWND,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        lpDirectory: std::ptr::null(),
        nShow: SW_HIDE,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        hIconOrMonitor: std::ptr::null_mut(),
        hProcess: std::ptr::null_mut(),
    };
    // SAFETY: `info` e os buffers UTF-16 vivem até o fim da chamada; o handle é fechado abaixo.
    unsafe {
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err("a permissão de administrador foi negada".into());
        }
        // WAIT_TIMEOUT = 0x102
        if WaitForSingleObject(info.hProcess, ELEVATED_TIMEOUT_MS) == 0x102 {
            CloseHandle(info.hProcess);
            return Err("a operação com permissão de administrador demorou demais".into());
        }
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        Ok(code)
    }
}

/// Anexa ao console de quem chamou (cmd/PowerShell), sem sobrescrever redirecionamentos.
pub fn attach_parent_console() {
    // SAFETY: chamadas Win32 simples; handles novos são entregues ao processo.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let invalid = usize::MAX as HANDLE;
        for (which, name, access) in [
            (0xFFFF_FFF6u32, "CONIN$", 0xC000_0000u32),
            (0xFFFF_FFF5, "CONOUT$", 0xC000_0000),
            (0xFFFF_FFF4, "CONOUT$", 0xC000_0000),
        ] {
            let cur = GetStdHandle(which);
            if cur.is_null() || cur == invalid {
                let h = CreateFileW(
                    w(name).as_ptr(),
                    access,
                    3,
                    std::ptr::null_mut(),
                    3,
                    0,
                    std::ptr::null_mut(),
                );
                if h != invalid {
                    SetStdHandle(which, h);
                }
            }
        }
    }
}

/// Caixa modal presa à janela `owner`, sempre à frente (nunca some atrás dela).
pub fn message_box_owned(owner: HWND, text: &str, caption: &str, kind: u32) -> i32 {
    // MB_SETFOREGROUND (0x10000) | MB_TOPMOST (0x40000)
    // SAFETY: strings UTF-16 terminadas em zero vivem até o fim da chamada.
    unsafe {
        MessageBoxW(
            owner,
            w(text).as_ptr(),
            w(caption).as_ptr(),
            kind | 0x10000 | 0x40000,
        )
    }
}

/// Caixa de mensagem simples (o app não tem console para mostrar erros).
pub fn message_box(text: &str, caption: &str, kind: u32) -> i32 {
    // SAFETY: strings UTF-16 terminadas em zero vivem até o fim da chamada.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            w(text).as_ptr(),
            w(caption).as_ptr(),
            kind,
        )
    }
}

/// Abre uma URL no navegador padrão (só informativo: nada é instalado).
pub fn open_url(url: &str) {
    // SAFETY: strings UTF-16 terminadas em zero vivem até o fim da chamada.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            w("open").as_ptr(),
            w(url).as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}

// --- Pasta de estado do sistema (ADR-031): ProgramData, DACL protegida, dono Administradores.

#[repr(C)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// `FOLDERID_ProgramData` {62AB5D82-FDC1-4DC3-A9DD-070D1D495D97}.
pub const FOLDERID_PROGRAMDATA: GUID = GUID {
    data1: 0x62AB_5D82,
    data2: 0xFDC1,
    data3: 0x4DC3,
    data4: [0xA9, 0xDD, 0x07, 0x0D, 0x1D, 0x49, 0x5D, 0x97],
};

#[repr(C)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: u32,
    pub lpSecurityDescriptor: *mut c_void,
    pub bInheritHandle: i32,
}

pub const SDDL_REVISION_1: u32 = 1;
pub const SE_FILE_OBJECT: u32 = 1;
pub const OWNER_SECURITY_INFORMATION: u32 = 1;

#[link(name = "shell32")]
extern "system" {
    pub fn SHGetKnownFolderPath(
        rfid: *const GUID,
        flags: u32,
        token: HANDLE,
        path: *mut *mut u16,
    ) -> i32;
}

#[link(name = "ole32")]
extern "system" {
    pub fn CoTaskMemFree(p: *mut c_void);
}

#[link(name = "kernel32")]
extern "system" {
    pub fn CreateDirectoryW(path: *const u16, sa: *const SECURITY_ATTRIBUTES) -> i32;
    pub fn LocalFree(h: *mut c_void) -> *mut c_void;
}

#[repr(C)]
pub struct LUID {
    pub LowPart: u32,
    pub HighPart: i32,
}

#[repr(C)]
pub struct TOKEN_PRIVILEGES {
    pub PrivilegeCount: u32,
    pub Luid: LUID,
    pub Attributes: u32,
}

pub const TOKEN_ADJUST_PRIVILEGES: u32 = 0x20;
pub const TOKEN_QUERY: u32 = 0x8;
pub const SE_PRIVILEGE_ENABLED: u32 = 2;
pub const DACL_SECURITY_INFORMATION: u32 = 4;
pub const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentProcess() -> HANDLE;
}

// Abrir o diretório de estado sem seguir ponto de reparse e mexer no descritor pelo HANDLE.
pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_SHARE_ALL: u32 = 7; // READ | WRITE | DELETE
pub const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
pub const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
pub const DELETE: u32 = 0x0001_0000;
pub const READ_CONTROL: u32 = 0x0002_0000;
pub const WRITE_DAC: u32 = 0x0004_0000;
pub const WRITE_OWNER: u32 = 0x0008_0000;
pub const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
/// `FILE_INFO_BY_HANDLE_CLASS::FileDispositionInfoEx`.
pub const FILE_DISPOSITION_INFO_EX: i32 = 21;
/// `FILE_DISPOSITION_FLAG_DELETE | POSIX_SEMANTICS | IGNORE_READONLY_ATTRIBUTE`.
pub const FILE_DISPOSITION_UNLINK_NOW: u32 = 0x1 | 0x2 | 0x10;

#[repr(C)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
pub struct BY_HANDLE_FILE_INFORMATION {
    pub dwFileAttributes: u32,
    pub ftCreationTime: FILETIME,
    pub ftLastAccessTime: FILETIME,
    pub ftLastWriteTime: FILETIME,
    pub dwVolumeSerialNumber: u32,
    pub nFileSizeHigh: u32,
    pub nFileSizeLow: u32,
    pub nNumberOfLinks: u32,
    pub nFileIndexHigh: u32,
    pub nFileIndexLow: u32,
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetFileInformationByHandle(h: HANDLE, info: *mut BY_HANDLE_FILE_INFORMATION) -> i32;
    pub fn SetFileInformationByHandle(h: HANDLE, class: i32, info: *const c_void, size: u32)
        -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn SetSecurityInfo(
        h: HANDLE,
        object_type: u32,
        info: u32,
        owner: *mut c_void,
        group: *mut c_void,
        dacl: *mut c_void,
        sacl: *mut c_void,
    ) -> u32;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> i32;
    pub fn LookupPrivilegeValueW(system: *const u16, name: *const u16, luid: *mut LUID) -> i32;
    pub fn AdjustTokenPrivileges(
        token: HANDLE,
        disable_all: i32,
        new_state: *const TOKEN_PRIVILEGES,
        buffer_len: u32,
        previous: *mut c_void,
        return_len: *mut u32,
    ) -> i32;
    /// Só os testes de ACL real (trocar o dono de um arquivo de teste); a produção usa o handle.
    #[cfg(test)]
    pub fn SetNamedSecurityInfoW(
        name: *const u16,
        object_type: u32,
        info: u32,
        owner: *mut c_void,
        group: *mut c_void,
        dacl: *mut c_void,
        sacl: *mut c_void,
    ) -> u32;
    pub fn GetSecurityDescriptorOwner(
        sd: *mut c_void,
        owner: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    pub fn GetSecurityDescriptorDacl(
        sd: *mut c_void,
        present: *mut i32,
        dacl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    /// `RtlGenRandom`: bytes aleatórios criptográficos.
    #[link_name = "SystemFunction036"]
    pub fn RtlGenRandom(buf: *mut u8, len: u32) -> u8;
    pub fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        sd: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    pub fn GetNamedSecurityInfoW(
        name: *const u16,
        object_type: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        sd: *mut *mut c_void,
    ) -> u32;
    pub fn ConvertSidToStringSidW(sid: *mut c_void, s: *mut *mut u16) -> i32;
}

// N1: arquivos do estado criados com DACL protegida explícita e conferência da DACL.
pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const GENERIC_READ: u32 = 0x8000_0000;
pub const FILE_SHARE_READ: u32 = 1;
pub const CREATE_NEW: u32 = 1;
pub const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
pub const SE_DACL_PROTECTED: u16 = 0x1000;
pub const ACL_SIZE_INFORMATION_CLASS: u32 = 2;

#[repr(C)]
pub struct ACL_SIZE_INFORMATION {
    pub AceCount: u32,
    pub AclBytesInUse: u32,
    pub AclBytesFree: u32,
}

#[link(name = "advapi32")]
extern "system" {
    pub fn GetSecurityDescriptorControl(sd: *mut c_void, control: *mut u16, rev: *mut u32) -> i32;
    pub fn GetAclInformation(acl: *mut c_void, info: *mut c_void, len: u32, class: u32) -> i32;
    pub fn GetSecurityInfo(
        h: HANDLE,
        object_type: u32,
        info: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        sd: *mut *mut c_void,
    ) -> u32;
    pub fn GetAce(acl: *mut c_void, index: u32, ace: *mut *mut c_void) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_troca_canais() {
        assert_eq!(rgb(0x112233), 0x0033_2211);
    }

    #[test]
    fn hkey_current_user_tem_sign_extension() {
        assert_eq!(HKEY_CURRENT_USER as i64 as u64, 0xFFFF_FFFF_8000_0001u64);
        assert_eq!(
            0x8000_0001u32 as i32 as i64 as u64,
            0xFFFF_FFFF_8000_0001u64
        );
    }

    #[test]
    fn w_termina_em_zero() {
        assert_eq!(w("ab"), vec![97, 98, 0]);
    }
}

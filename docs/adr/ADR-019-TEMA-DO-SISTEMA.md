# ADR-019 — Sincronização Automática com o Tema do Sistema (Windows 11 Dark/Light + Linux Portal/GTK)

- **Status:** Aceito
- **Data:** 08/11/2026
- **Autor:** `gl-arquiteto`
- **Alvo:** `guialar-rs`

---

## 1. Problema Diagnosticado

Na validação visual da `v0.3.0` na VM Windows 11 (`win11`), com o tema escuro do Windows 11 ativo em *Settings → Personalization*, a janela do GuiaLar Digital permaneceu branca (tema claro) e exibia um botão manual `"Alternar tema"`. No Linux em ambientes sem portal GNOME/KDE (como XFCE no Fedora/Void), o `libadwaita` também ignora temas GTK `-dark` se não for instruído via `AdwStyleManager`.

### Causas-Raiz no Windows (`src/ui/windows_winui.rs` e `src/win32.rs`)
1. **Bug de Sign-Extension em 64 bits no `HKEY_CURRENT_USER`:**
   - `windows_winui.rs:67` passava `0x8000_0001u32 as isize`, que em `x86_64` resulta em `0x0000_0000_8000_0001` (handle inválido — `ERROR_INVALID_HANDLE = 6`).
   - Na ABI Win64, `HKEY_CURRENT_USER` é `(HKEY)(ULONG_PTR)((LONG)0x80000001)` = `0xFFFF_FFFF_8000_0001` (`0x8000_0001u32 as i32 as isize`).
   - Por isso, `RegGetValueW` falhava sempre e `system_prefers_dark()` retornava `false` incondicionalmente.
2. **Controles Win32 (`BUTTON`) e Barra de Título no Modo Escuro:**
   - Para que `SetWindowTheme(btn, L"DarkMode_Explorer", NULL)` funcione nos botões Win32 e nos diálogos, é necessário habilitar o modo escuro no processo via `uxtheme.dll` (ordinais `135` `SetPreferredAppMode(1 = AllowDark)`, `133` `AllowDarkModeForWindow(hwnd, true)` e `136` `FlushMenuThemes()`) **ou** desenhar os botões com estilo Fluent/Dark (ou ambos).
   - Además, definir `DWMWA_USE_IMMERSIVE_DARK_MODE` (atributo `20` e fallback `19`) **e** `DWMWA_CAPTION_COLOR` (atributo `35` com `rgb(p.background)`) garante que a barra de título fique escura mesmo em VMs QEMU com Basic Display Adapter (sem GPU 3D).
3. **Botão manual `"Alternar tema"` desnecessário:**
   - O aplicativo deve seguir automaticamente o tema do sistema operacional tanto na abertura quanto ao vivo quando o usuário troca o tema nas configurações do SO (`WM_SETTINGCHANGE` / `WM_THEMECHANGED` no Windows; `notify::gtk-theme-name` + portal XDG no Linux), sem botão manual `"Alternar tema"`.

---

## 2. Especificação Obrigatória de Implementação

### 2.1. `src/win32.rs`
1. Definir a constante correta:
   ```rust
   pub const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
   pub const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
   ```
2. Adicionar teste unitário em `src/win32.rs` (ou `src/ui/theme.rs` para rodar também em host Linux!):
   - Garantir que `(0x8000_0001u32 as i32 as i64 as u64) == 0xFFFF_FFFF_8000_0001u64`.
3. Carregar funções de `uxtheme.dll` via `GetModuleHandleW(w("uxtheme.dll"))` + `GetProcAddress` cacheados em um `OnceLock` (evitando `LoadLibraryW` repetido sem `FreeLibrary`, já que `#[link(name = "uxtheme")]` garante a DLL carregada):
   - Ordinal `135` de `uxtheme.dll`: `SetPreferredAppMode(1)` (`AllowDark`)
   - Ordinal `133` de `uxtheme.dll`: `AllowDarkModeForWindow(hwnd, 1)`
   - Ordinal `136` de `uxtheme.dll`: `FlushMenuThemes()`

### 2.2. `src/ui/windows_winui.rs`
1. **Leitura confiável do Registro do Windows:**
   - Usar `HKEY_CURRENT_USER` (`0x8000_0001u32 as i32 as isize`).
   - Ler `AppsUseLightTheme` em `Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`; se ausente/erro, ler `SystemUsesLightTheme`. Retornar `true` (escuro) quando o valor for `0`.
2. **Remover o botão `"Alternar tema"` (`ID_THEME`) e `USER_TOGGLED`:**
   - Manter apenas os 4 botões dos cartões (`ID_ACTIVATE`, `ID_BROWSERS`, `ID_CHECK`, `ID_RESTORE`), usando `.iter()` (sem `.enumerate()` residual) em `set_busy`.
   - Assim o cabeçalho fica limpo e o app segue 100% o sistema.
3. **Barra de título, fundo e botões escuros (`apply_theme`):**
   - Passar `DwmSetWindowAttribute(hwnd, 20, &dark, 4)` e `DwmSetWindowAttribute(hwnd, 19, &dark, 4)`.
   - Passar `DwmSetWindowAttribute(hwnd, 35, &caption_color, 4)` (`DWMWA_CAPTION_COLOR` = `rgb(p.background)`) e `DwmSetWindowAttribute(hwnd, 36, &text_color, 4)` (`DWMWA_TEXT_COLOR` = `rgb(p.text)`), garantindo que a barra de título do Windows 11 fique `#202020` com texto branco no tema escuro mesmo sob renderização de software do QEMU.
   - Chamar `AllowDarkModeForWindow` + `SetWindowTheme(*b, w("DarkMode_Explorer" ou "Explorer"), null)` e `InvalidateRect(*b, null, 1)` em cada botão.
4. **Reação imediata ao mudar tema no Windows (`WM_SETTINGCHANGE` `0x001A` e `WM_THEMECHANGED` `0x031A`):**
   - Sempre atualizar `DARK.store(system_prefers_dark(), Ordering::Relaxed)`, chamar `apply_theme(hwnd)` e `InvalidateRect(hwnd, null, 1)`.

### 2.3. `src/ui/linux_gtk.rs`
1. **Atenção ao comportamento do `adw_init` (achado do `gl-reviewer`):**
   - O `adw_init()` sobrescreve a propriedade `GtkSettings:gtk-theme-name` para `"Adwaita-empty"` com prioridade de aplicação. Portanto, ler `GtkSettings:gtk-theme-name` **depois** de `adw_init()` sempre retorna `"Adwaita-empty"`.
   - Para detectar corretamente o tema em ambientes como XFCE/MATE/GNOME sem depender de `GtkSettings` pós-`adw_init`:
     - **Fonte 1:** Variável de ambiente `GTK_THEME` (se definida).
     - **Fonte 2:** `GSettings` (`org.gnome.desktop.interface`, chaves `"color-scheme"` — ex. `"prefer-dark"` — e `"gtk-theme"` via `g_settings_schema_source_get_default` / `g_settings_schema_source_lookup` / `g_settings_new` / `g_settings_get_string` em `libgio-2.0.so.0`, para não abortar se o schema não existir) **e/ou** leitura de `gtk_init()` + `GtkSettings:gtk-theme-name` **antes** de `adw_init()` (guardando o tema inicial) + fallback para `~/.config/gtk-3.0/settings.ini` ou `xfconf` se aplicável.
     - Se usar `g_object_get`, declará-la como função variádica C (`unsafe extern "C" fn(P, *const c_char, ...)`).
2. **Constante correta do `AdwColorScheme`:**
   - `ADW_COLOR_SCHEME_DEFAULT = 0`
   - `ADW_COLOR_SCHEME_PREFER_DARK = 3` (ou `ADW_COLOR_SCHEME_FORCE_DARK = 4` quando `GTK_THEME` / `gtk-theme` explícito for `-dark` e não houver portal respondendo; usar `PREFER_DARK = 3` ou `FORCE_DARK = 4` documentando claramente).
3. **Troca ao vivo no Linux:**
   - Manter uma instância viva de `GSettings` (`org.gnome.desktop.interface`, quando o schema existir) conectada aos sinais `"changed::gtk-theme"` e `"changed::color-scheme"` para chamar `sync_system_color_scheme` ao vivo.

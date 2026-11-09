# ADR-021: Integração no Linux (menu de aplicativos e instalador MVP)

## Contexto
O GuiaLar Digital no Linux é distribuído como AppImage (`guialar-rs/target/release/bundle/appimage/guialar-*-x86_64.AppImage`). O AppImage contém um arquivo `.desktop` interno, mas **não se integra sozinho** ao menu de aplicativos do desktop environment (GNOME, KDE, XFCE, etc.). O gl-usuario (seq 1033, 1046) não encontrou o programa no Fedora; precisou abrir pelo gerenciador de arquivos.

Critério de publicação exige que um usuário leigo consiga instalar e achar o app sem terminal.

## Decisão
**MVP: integração no primeiro run (auto-instalação silenciosa).**

1. **No primeiro `apply` bem-sucedido** (ou no primeiro `gui` se não houver `apply` prévio), o binário:
   - Extrai seu `.desktop` interno para `~/.local/share/applications/guialar-digital.desktop`
   - Ajusta `Exec=` para o caminho real do AppImage (ou do binário instalado, se houver instalador futuro)
   - Chama `xdg-desktop-menu install --novendor ~/.local/share/applications/guialar-digital.desktop` (ignora falha silenciosamente)
   - Atualiza o cache de ícones: `gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor` (ignora falha)

2. **Fallback**: se `xdg-desktop-menu` não existir ou falhar, o `.desktop` fica em `~/.local/share/applications/` — a maioria dos DEs modernos o lê direto de lá.

3. **Não faz**: rpm/deb/flatpak/snap, atalhos na área de trabalho, entry no `PATH`, daemon de auto-update. Isso fica para ADR futuro.

4. **Idempotência**: reexecutar a integração não duplica entradas; o `.desktop` tem `StartupNotify=true` e `X-GUILAR-AUTO=1` como marca.

## Consequências
- **Positivo**: usuário leigo abre o menu, acha "GuiaLar Digital", clica e funciona. Zero configuração manual.
- **Negativo**: depende de `xdg-desktop-menu` (xdg-utils) instalado — presente na maioria das distros desktop. Em ambientes mínimos (servidor, container) a integração falha silenciosamente, o que é aceitável (não há menu).
- **Risco**: se o AppImage for movido/deletado, o `.desktop` aponta para caminho inválido. Mitigação: no `gui` ou `apply`, se o caminho no `.desktop` não existir, reescreve com o caminho atual.

## Implementação
- Módulo: `guialar_core::linux_integration` (novo)
- Função pública: `ensure_desktop_integration(appimage_path: &Path) -> Result<(), IntegrationError>`
- Chamada: no `gui_main()` antes de mostrar a janela, e no `apply` após sucesso.
- Testes unitários: mock de `std::fs`, verifica escrita do `.desktop` e chamada de `xdg-desktop-menu` (via `std::process::Command` mockado).

## Alternativas consideradas
| Opção | Por que não |
|-------|-------------|
| Instalador nativo (rpm/deb) | Escopo grande, manutenção por distro, não é MVP |
| `appimaged` / `AppImageLauncher` daemon | Dependência externa, não controlamos |
| Script `install.sh` separado | Usuário leigo não roda script |
| Entrada no `PATH` via symlink | Não resolve menu; polui `~/.local/bin` |

## Referências
- freedesktop.org Desktop Entry Specification
- `xdg-desktop-menu(1)`, `xdg-icon-resource(1)`
- ADR-018 (GTK 4 / Libadwaita por dlopen)
- ADR-020 (ergonomia: estados, ações, acessibilidade)

## Status
Proposto. Aguarda aprovação do lead (gl-arquiteto) para implementação pelo gl-refatorador.
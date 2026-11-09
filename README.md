# GuiaLar Digital v0.3.0

Proteção familiar de navegação via **DNS Cloudflare for Families** (1.1.1.3 / 1.0.0.3) e instalador do **uBlock Origin** para Linux e Windows.

Reescrita em Rust com GUI nativa: **GTK 4 / Libadwaita** no Linux (dlopen, sem dependência de runtime) e **Win32 / Fluent com Mica** no Windows.

## Instalação

### Linux (AppImage)
```bash
# Baixe a AppImage mais recente do GitHub Releases
chmod +x GuiaLar-Digital-*.AppImage
./GuiaLar-Digital-*.AppImage
```
No primeiro uso, o programa se integra automaticamente ao menu de aplicativos.

### Windows (Instalador)
Baixe o `.exe` do GitHub Releases e execute. Requer elevação (UAC) para aplicar a proteção.

## Uso

### Interface gráfica
Execute sem argumentos para abrir a GUI:
- Mostra estado atual: **Protegido** / **Desprotegido** / **Parcial** / **Trabalhando**
- Ações disponíveis mudam conforme o estado (máximo 1 primária + 1 secundária)
- **Enter** = ação primária | **Esc** = cancela ação em andamento | **Ctrl+R** = verifica status
- Botão **Detalhes técnicos** (recolhível): IPs, manifesto, logs, botão **Copiar detalhes**

### Linha de comando
```bash
guialar status     # Consulta status (protegido/não protegido/parcial + IPs DNS atuais)
guialar apply      # Ativa proteção familiar (requer sudo/UAC)
guialar revert     # Restaura configuração anterior
guialar browser    # Abre loja de extensões para instalar uBlock Origin
guialar --help     # Ajuda completa
```

## Requisitos

| Plataforma | Requisitos |
|------------|------------|
| Linux | NetworkManager **ou** systemd-resolved **ou** runit/resolvconf; GTK 4 + Libadwaita (carregados via dlopen — se ausentes, cai para CLI) |
| Windows 10/11 | PowerShell 5.1+; UAC para `apply`/`revert` |

## Compilação (desenvolvedores)

```bash
# Linux
cargo build --release
# AppImage: cargo bundle --release (requer cargo-bundle)

# Windows (cross-compile a partir do Linux)
cargo build --release --target x86_64-pc-windows-gnu
# Ou nativo no Windows: cargo build --release
```

## Distribuição

- **Linux**: AppImage standalone (GLIBC ≥ 2.34, usa libs GTK do sistema)
- **Windows**: Executável PE32+ GUI assinado (futuro: certificado EV para SmartScreen)
- **CI**: `cargo-dist` para builds automáticos e releases no GitHub (branch `rust`)

## Licença

MIT — veja [LICENSE](LICENSE).

## Autor

Guilherme Amaral Colamarco Resende de Melo — <guicolamarco150@gmail.com>

Projeto acadêmico: **Projetos Integrados I** (GuiaLar Digital).
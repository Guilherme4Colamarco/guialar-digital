# GuiaLar Digital v0.3.0

Proteção de navegação por **DNS filtrado**, com duas opções:

| Modo | O que bloqueia | DNS usado |
|------|----------------|-----------|
| **Proteção familiar** (padrão) | Malware e conteúdo adulto | Cloudflare for Families (`1.1.1.3` / `1.0.0.3`) |
| **Só bloquear anúncios** | Anúncios e rastreadores | AdGuard DNS Padrão (`94.140.14.14` / `94.140.15.15`) |

O modo familiar também configura o bloqueador de anúncios uBlock Origin nos navegadores (no Windows, por política; no Linux, abre a página da extensão). O modo "só anúncios" não mexe nos navegadores. Desativar a proteção devolve o DNS exatamente ao que era antes, mesmo que o arquivo de estado seja apagado ou corrompido.

> **Limites do filtro por DNS.** Ele bloqueia por **domínio**: não pega anúncio servido pelo mesmo domínio do site, nem navegador ou app com DNS próprio (DoH), nem tráfego que sai por uma VPN. Se a rede já tem outro bloqueador (um Pi-hole, por exemplo), o bloqueio pode vir dele e não do GuiaLar; por isso o programa só diz "Protegido" quando o DNS configurado é o do modo escolhido e o teste de bloqueio responde.

Reescrita em Rust com interface nativa: **GTK 4 / Libadwaita** no Linux (carregado em tempo de execução) e **Win32 / Fluent (Mica)** no Windows. Há também uma seção informativa de **navegadores recomendados** (Brave, Helium, Zen Browser + uBlock Origin, Mullvad Browser, LibreWolf); o programa só mostra os links oficiais e não instala nada.

## Instalação

### Linux (AppImage)
```bash
chmod +x GuiaLar-Digital-*.AppImage
./GuiaLar-Digital-*.AppImage
```
No primeiro uso, o programa cria a entrada no menu de aplicativos. Pede senha de administrador (polkit ou `sudo`) só ao ativar ou desativar.

### Windows
Baixe o `guialar.exe` e execute (arquivo único, não precisa instalar). O Windows pede confirmação de administrador (UAC) ao ativar ou desativar. O executável ainda **não é assinado**: o SmartScreen pode mostrar "editor desconhecido".

## Uso

### Interface gráfica
Execute sem argumentos:
- O estado é **Desprotegido**, **Protegido**, **Parcial** ou **Trabalhando**; os botões mudam conforme o estado (uma ação principal e uma secundária).
- Desprotegido: escolha o modo (familiar ou só anúncios) e clique em **Proteger este computador**.
- **Ctrl+R** verifica o estado. **Desativar proteção** pede confirmação.
- **Detalhes técnicos** (recolhível) mostra IPs, caminho do estado e o botão **Copiar detalhes**.

### Linha de comando
```bash
guialar status                 # estado e filtro ativo
guialar apply                  # filtro familiar (padrão)
guialar apply --mode ads       # só anúncios (troca de modo mantém o DNS original)
guialar setup [--mode ads]     # tudo de uma vez (filtro + navegadores no modo familiar)
guialar revert                 # volta ao DNS de antes
guialar browser                # bloqueador de anúncios nos navegadores
guialar --help
```

## Requisitos

| Plataforma | Requisitos |
|------------|------------|
| Linux | NetworkManager, systemd-resolved ou runit/resolvconf. GTK 4 + Libadwaita para a janela (sem eles, usa a linha de comando). glibc ≥ 2.34 |
| Windows 10/11 | PowerShell 5.1+ |

## Compilação (desenvolvedores)

```bash
cargo build --release                                   # Linux
cargo build --release --target x86_64-pc-windows-gnu    # Windows (cross-compile)
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
O binário Linux usa glibc (necessário para carregar GTK em tempo de execução); o AppImage é montado a partir dele.

## Licença

Licença dupla: [MIT](LICENSE-MIT) ou [Apache-2.0](LICENSE-APACHE), à sua escolha.

## Autor

Guilherme Amaral Colamarco Resende de Melo — <88679520+Guilherme4Colamarco@users.noreply.github.com>

Projeto acadêmico: **Projetos Integrados I** (GuiaLar Digital).
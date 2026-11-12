# GuiaLar Digital v0.3.0

Proteção de navegação por **DNS filtrado**, com duas opções:

| Modo | O que bloqueia | DNS usado |
|------|----------------|-----------|
| **Proteção familiar** (padrão) | Malware e conteúdo adulto | Cloudflare for Families (`1.1.1.3` / `1.0.0.3`) |
| **Só bloquear anúncios** | Anúncios e rastreadores | AdGuard DNS Padrão (`94.140.14.14` / `94.140.15.15`) |

O modo familiar também cuida do bloqueador de anúncios nos navegadores. **No Windows**, o programa o instala por política: o **uBlock Origin Lite** no Chrome, no Edge e no Brave, e o **uBlock Origin** no Firefox; ele aparece quando o navegador é reaberto. **No Linux**, o botão **Proteger este computador** só muda o DNS: o bloqueador é um passo à parte (`guialar setup` ou `guialar browser`), que abre a página da extensão na loja de cada navegador (uBlock Origin Lite no Chrome, Chromium, Brave e Edge; uBlock Origin no Firefox). O modo "só anúncios" não mexe nos navegadores.

Desativar a proteção devolve o DNS que estava antes. Se o registro da proteção (`state.json`, veja onde fica em «Desfazer e remover») for apagado ou estragar, o programa não sabe mais o que havia antes: o DNS volta ao **automático da rede**, e um DNS fixo que você tinha configurado à mão precisa ser colocado de novo.

> **Limites do filtro por DNS.** Ele bloqueia por **domínio**: não pega anúncio servido pelo mesmo domínio do site, nem navegador ou app com DNS próprio (DoH), nem tráfego que sai por uma VPN. Se a rede já tem outro bloqueador (um Pi-hole, por exemplo), o bloqueio pode vir dele e não do GuiaLar; por isso o programa só diz "Protegido" quando o DNS configurado é o do modo escolhido e o teste de bloqueio responde.

> **Limites do bloqueador nos navegadores (Windows).** O Chromium não é detectado nem configurado (só Chrome, Edge, Brave e Firefox). O programa grava a política, mas a extensão só aparece quando o navegador é reaberto.

Reescrita em Rust com interface nativa: **GTK 4 / Libadwaita** no Linux (carregado em tempo de execução) e **Win32 / Fluent (Mica)** no Windows. Há também uma seção informativa de **navegadores recomendados** (Brave, Helium, Zen Browser + uBlock Origin, Mullvad Browser, LibreWolf); o programa só mostra os links oficiais e não instala nada.

## Instalação

Baixe os arquivos da [página de Releases](https://github.com/Guilherme4Colamarco/guialar-digital/releases). Os nomes seguem a versão: `guialar-<versão>-x86_64.AppImage` (Linux) e `guialar-<versão>-windows.exe` (Windows). O arquivo `SHA256SUMS` da mesma página permite conferir o download.

### Linux (AppImage)
```bash
chmod +x guialar-*-x86_64.AppImage
./guialar-*-x86_64.AppImage
```
No primeiro uso, o programa cria a entrada no menu de aplicativos. Pede senha de administrador (polkit ou `sudo`) só ao ativar ou desativar.

### Windows
Baixe o `guialar-<versão>-windows.exe` e execute (arquivo único, não precisa instalar). O Windows pede confirmação de administrador (UAC) ao ativar, desativar ou configurar os navegadores. O executável ainda **não é assinado**: o SmartScreen pode mostrar "editor desconhecido".

## Uso

### Interface gráfica
Execute sem argumentos:
- O estado é **Desprotegido**, **Protegido**, **Parcial** ou **Trabalhando**; os botões mudam conforme o estado (uma ação principal e uma secundária).
- Desprotegido: escolha o modo (familiar ou só anúncios) e clique em **Proteger este computador**. A janela mostra o DNS de agora e o novo, e pede confirmação antes de mudar.
- **Ctrl+R** verifica o estado. **Desativar proteção** também pede confirmação.
- **Detalhes técnicos** (recolhível) mostra IPs, caminho do estado e o botão **Copiar detalhes**.

### Linha de comando
```bash
guialar status                 # estado e filtro ativo
guialar apply                  # filtro familiar (padrão)
guialar apply --mode ads       # só anúncios (troca de modo mantém o DNS original)
guialar setup [--mode ads]     # tudo de uma vez (filtro + navegadores no modo familiar)
guialar revert                 # volta ao DNS de antes
guialar browser [--install-all]  # bloqueador de anúncios nos navegadores
guialar --help
```

### Desfazer e remover
- **Desativar proteção** (janela) ou `guialar revert` devolve o DNS. No Windows, também tira o bloqueador de anúncios que o programa instalou nos navegadores. No Linux, **não** remove a extensão que você instalou na loja do navegador.
- Para **remover o programa**: no Linux, apague o AppImage, o atalho `~/.local/share/applications/guialar-digital.desktop` e o ícone `~/.local/share/icons/hicolor/256x256/apps/br.uniube.pi.guialar.png`. No Windows, apague o `.exe` (nada é instalado).
- Desative a proteção **antes** de apagar o programa, para o DNS voltar ao que era.

### Onde o programa guarda o registro e como apagá-lo
O registro que permite desfazer (`state.json`, só com endereços de DNS e nomes de conexão) fica numa pasta **do sistema**, gravada apenas pelo programa com permissão de administrador; o seu usuário só a lê:
- **Linux:** `/var/lib/guialar/state.json`. A trava que impede duas operações ao mesmo tempo fica em `/run/guialar/op.lock` (some ao reiniciar). Para apagar o registro: `sudo rm -rf /var/lib/guialar`.
- **Windows:** `%ProgramData%\GuiaLar\state.json` (e `op.lock` na mesma pasta), com acesso de escrita só para administradores. Para apagar, como administrador: `rmdir /s /q "%ProgramData%\GuiaLar"`.

Apagar o registro **não desfaz o DNS**: faça isso depois de desativar a proteção (ou, sem o registro, rode `guialar revert`, que volta o DNS ao automático da rede). Quem usou a versão beta tinha o registro na pasta pessoal (`~/.config/guialar` no Linux, `%LOCALAPPDATA%\GuiaLar` no Windows): ao desativar, o programa lê esse registro antigo uma vez, confere se ainda faz sentido e o apaga; se estiver estranho, ignora e volta o DNS ao automático.

## Requisitos

Funciona nas distribuições Linux comuns (Ubuntu, Fedora, Mint…) e no Windows 10/11. Em detalhe:

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
O binário Linux usa glibc (necessário para carregar GTK em tempo de execução); o AppImage é montado a partir dele. A compilação cruzada para Windows precisa do `llvm-windres` (ou de outro `windres`, indicado em `GUIALAR_WINDRES`): sem ele, o build falha ao compilar os recursos do `.exe`.

## Licença

Licença dupla: [MIT](LICENSE-MIT) ou [Apache-2.0](LICENSE-APACHE), à sua escolha.

## Autor

Guilherme Amaral Colamarco Resende de Melo — <88679520+Guilherme4Colamarco@users.noreply.github.com>

Projeto acadêmico: **Projetos Integrados I** (GuiaLar Digital).
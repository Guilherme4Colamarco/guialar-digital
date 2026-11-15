# Changelog

## 0.4.1 — 2026-11-15

### Janelas
- Linux (GTK): com «Detalhes técnicos» aberto, a página de estado não encolhe mais: o escudo e o título ficam inteiros e quem rola é a janela.
- Linux (GTK): o escudo e o título ficam na cor do estado (verde, amarelo, vermelho), como no Windows.
- Windows: layout mais compacto; o texto dos detalhes aparece inteiro (o botão «Copiar detalhes» passou para a linha dos botões) e a janela não passa da área útil da tela.
- Linux: os textos não dizem mais que o programa instala ou remove o bloqueador de anúncios; ele só abre a página da extensão na loja.

## 0.4.0 — 2026-11-15

> **Ainda na beta (`beta-06af688`)?** Desative a proteção com a 0.3.x **antes** de atualizar. A 0.4.0 não lê mais o registro que a beta guardava na pasta pessoal: sem ele, o desfazer devolve o DNS ao automático da rede, e um DNS fixo que você tinha configurado precisa ser colocado de novo.

### Proteção
- Linux com systemd-resolved (Fedora, Ubuntu e derivados): o filtro passa a ser **global** (drop-in com `Domains=~.`), mesmo em máquinas com NetworkManager. Antes ele era gravado em cada conexão ativa na hora de proteger, e uma rede Wi-Fi nova ou o hotspot de um celular entrava **sem filtro** (ADR-032). Quem já está protegido precisa desativar e proteger de novo para passar ao filtro global.
- Limite novo: com o filtro ligado, nomes internos de uma VPN de trabalho podem deixar de resolver. Em Linux com NetworkManager **sem** systemd-resolved a brecha da conexão nova continua.

### Mudou
- `guialar apply` e `guialar setup` só dizem «ativada» depois de conferir que o filtro responde, e saem com código 1 quando o DNS foi gravado mas a proteção não ficou completa. Antes imprimiam «ativada com sucesso» e saíam com 0 mesmo assim.
- Proteger, trocar de modo e desfazer passam por um módulo só (`src/protection.rs`), usado pela linha de comando, pelas janelas e pelo processo elevado. O resultado é um tipo, não um texto: a janela deixou de reler o registro por conta própria para saber se estava danificado.

### Removido
- Migração do registro da beta (pasta pessoal), como previsto na ADR-031. Com ela saem a leitura sem seguir links na área do usuário no Linux e a identificação do usuário por trás do `sudo`/`pkexec`, que só a migração usava. O processo elevado não lê mais nada que um usuário comum controle no Linux.
- Prefixo de elevação por comando (`sudo`/`pkexec` na frente de cada `nmcli`, `tee`...): desde que o programa passou a se reexecutar elevado uma única vez, o prefixo era sempre vazio.
- Dependência `thiserror`, que não era usada.

### Build
- `[profile.release]` com `strip`, LTO e `codegen-units = 1`.
- A versão nos recursos do `.exe` (Propriedades do arquivo) volta a acompanhar a do programa; a 0.3.1 ainda dizia 0.3.0.

### Windows
- Segurança: `reg`, `powershell`, `ipconfig`, `cmd` e `clip` passam a ser chamados pelo caminho completo da pasta do sistema (lida pela API, não por variável de ambiente). Antes eram chamados só pelo nome, e o Windows procura primeiro na pasta do próprio programa: um `reg.exe` deixado ao lado do `guialar.exe` era executado pelo processo elevado. Medido na 0.3.1 numa VM Windows 11 com `setup --mode family`. A correção ainda não foi executada no Windows de verdade (só compila e passa no lint para o alvo Windows).
- Segurança: o `.exe` passa a buscar DLLs só na pasta do sistema (`/DEPENDENTLOADFLAG:0x800` no link e `SetDefaultDllDirectories` no início do programa). Antes, uma `dwmapi.dll` ou `UxTheme.dll` deixada ao lado do `guialar.exe` seria carregada pelo processo elevado. A CI recusa um `.exe` sem essa marca.
- Segurança: `Program Files` e `LocalAppData` passam a vir da API do Windows (`SHGetKnownFolderPath`), não de variáveis de ambiente que quem chama o processo elevado controla.

## 0.3.1 — 2026-11-13

### Windows
- O `.exe` da 0.3.0 não abria em Windows sem o pacote Visual C++ instalado (erro «VCRUNTIME140.dll não encontrado»). Agora a biblioteca do C vai embutida no `.exe`, e a CI recusa publicar um `.exe` que dependa dela.
- Nenhuma mudança de comportamento. O AppImage do Linux é o mesmo código da 0.3.0.

## 0.3.0 — 2026-11-13

Primeira versão pública em Rust. Substitui o Java na `main`. O Java permanece na branch `legacy` e na tag `java-final` (a tag será criada na publicação).

> **Vindo da beta?** O AppImage antigo da beta extraía uma cópia como root em `/tmp/appimage_extracted_<código>`. A 0.3.0 não faz mais isso, mas a sobra antiga pode continuar lá até reiniciar. Para apagar agora: `sudo rm -rf /tmp/appimage_extracted_*`.

### Proteção
- Dois modos de filtro DNS: **Proteção familiar** (Cloudflare for Families `1.1.1.3` / `1.0.0.3`) e **Só bloquear anúncios** (AdGuard DNS Padrão `94.140.14.14` / `94.140.15.15`).
- Desativar devolve o DNS original. Se o manifesto (`state.json`) sumir ou corromper, o fallback volta ao DNS automático da rede (no `resolv.conf` direto, remove só as linhas do GuiaLar).
- «Protegido» só aparece com o DNS do modo escolhido **e** a sonda de bloqueio respondendo (ADR-025). O programa mostra o DNS de agora e o novo antes de ativar ou trocar, e confirma antes de desativar (ADR-026).
- «Parcial» quando há DNS de fora ou ilegível; ativar de novo reaplica o filtro (ADR-027).
- NetworkManager: o revert devolve o DNS fixo que a conexão tinha (IPv4 e IPv6, com `ignore-auto-dns` sim ou não), em vez de zerá-lo.
- Estado e trava numa **pasta do sistema**, gravada só pelo processo elevado (ADR-031): Linux `/var/lib/guialar/` e `/run/guialar/op.lock`; Windows `%ProgramData%\GuiaLar\` com ACL só de administradores para escrita. O programa confere o dono antes de confiar no que lê, e o `apply` confere o estado **antes** de mexer no DNS. O registro da beta (na pasta pessoal) é validado e migrado uma vez no `revert`.
- Linux (AppImage): a elevação roda uma **cópia** do binário numa pasta privada do usuário (sem shell, sem extração como root, nunca no `/tmp`), apagada ao final; o atalho do menu só é reescrito pelo próprio AppImage, e `APPIMAGE` só vale com o executável dentro do `APPDIR` (ADR-028).
- Windows: o `policies.json` do Firefox não atravessa junção nem link, e o Firefox só é configurado em `Program Files` (ADR-030). A pasta de estado em `%ProgramData%\GuiaLar` é sempre refeita com a ACL protegida (mesmo se já existia com ACL herdada), uma pasta de outro dono é assumida (não travada por um handle aberto) e uma trava de dono alheio é ignorada.
- Windows: «DHCP ou fixo» passa a vir do `NameServer` do registro (IPv4 e IPv6), não do DNS efetivo. Antes, numa interface DHCP, desativar a proteção gravava o DNS do roteador como se fosse fixo e o notebook perdia a navegação em outra rede.
- Bloqueador de anúncios nos navegadores (Windows): uBlock Origin Lite no Chrome, Edge e Brave, com o ID de cada loja (o Edge usa o da loja do Edge); uBlock Origin no Firefox. Um valor `9101` de outra política não é sobrescrito, e a falha em um navegador não impede os outros. No Linux, a página da extensão abre na loja certa. Chromium no Windows não é detectado.
- «Sem permissão para ler o registro» agora aponta a pasta de estado de verdade (`/var/lib/guialar` no Linux, `%ProgramData%\GuiaLar` no Windows) em vez da pasta pessoal da beta, e não manda mais dar a posse ao usuário (o programa deixaria de confiar no registro).
- Linux: `apply` e troca de modo seguem a mesma ordem segura do Windows, agora escrita uma vez para todos os backends (NetworkManager, systemd-resolved, resolvconf e `resolv.conf`). O estado de **todas** as conexões é lido antes de alterar qualquer uma, o registro é gravado antes da primeira mudança, e uma falha no meio devolve o que já foi trocado (se o desfazer também falhar, o registro fica e a mensagem manda usar «Desativar»). Antes, o Wi-Fi caindo no meio do `apply` deixava a Ethernet no filtro sem registro, e o `revert` seguinte zerava o DNS fixo do usuário.
- Se o `apply` ou a troca de modo falha no meio, só as conexões já tocadas são desfeitas (NetworkManager e Windows): as outras não são religadas nem regravadas. Depois de um desfazer que deu certo o cache de DNS é limpo (Windows). Se as políticas dos navegadores não voltam, o registro grava o modo em que o DNS realmente ficou. O `resolv.conf` que não pode ser lido agora aborta o `apply` em vez de guardar um backup vazio, e o desfazer sem registro do NetworkManager tenta todas as conexões e junta as falhas.
- Windows: o `apply` passou a achar a interface pelo **nome** (`-InterfaceAlias`), não mais pelo índice (`-InterfaceIndex`), o mesmo da troca de modo. Ainda não testado no Windows de verdade (só compila e passa nos testes com runner falso).
- «Desativar» que não consegue apagar o registro depois de devolver o DNS agora só avisa, também no Linux (como já era no Windows).
- Estado «protegido, mas sem registro» e o `revert` usam a mesma leitura de DNS do estado e do `apply` (inclui IPv6 e `resolvectl`), em vez de uma leitura própria que divergia.
- Trava de operação (`OpLock`) com PID; trava órfã não prende o programa.
- Instância única da janela. Subcomandos internos exigem administrador.

### Interface
- Linux: GTK 4 + Libadwaita (carregados em tempo de execução).
- Windows: Win32 / Fluent. Executável GUI (sem console no duplo clique).
- Uma ação primária e uma secundária por estado (Desprotegido, Protegido, Parcial, Trabalhando).
- Seção informativa de navegadores recomendados (Brave, Helium, Zen + uBlock Origin, Mullvad Browser, LibreWolf). Não instala nada.

### Distribuição
- Linux: AppImage glibc (GTK do sistema via `dlopen`), `guialar-<versão>-x86_64.AppImage`. Integração no menu no primeiro uso (`ADR-021`).
- Windows: `guialar-<versão>-windows.exe`, arquivo único, ainda sem assinatura digital.

### Qualidade
- Suíte de testes automatizados em camadas (ADR-029): unitários, costura de comandos com `FakeRunner` (argv exato, `apply ∘ revert`, idempotência, UAC negado), integração com estado isolado e E2E em VM.

### CLI
```
guialar status
guialar apply [--mode family|ads]
guialar setup [--mode family|ads]
guialar revert
guialar browser [--install-all]
```

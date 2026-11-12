# Changelog

## 0.3.0 — a definir (data da tag)

Primeira versão pública em Rust. Substitui o Java na `main`. O Java permanece na branch `legacy` e na tag `java-final` (a tag será criada na publicação).

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

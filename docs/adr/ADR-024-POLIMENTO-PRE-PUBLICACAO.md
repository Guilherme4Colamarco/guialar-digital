# ADR-024 — Polimento antes de publicar (auditoria)

Status: Aceita (2026-11-10) · Autor: gl-arquiteto · Pedido: Guilherme (auditoria, seq 1408)

## Contexto
O Rust local (`3bc3b0b`) já corrige os três bugs da revisão do `b2bd998`. O gl-reviewer reviu esse commit e não deixou achado grave. A main do GitHub continua no Java. Esta auditoria não publica.

`check_status` (`src/verify.rs`) devolve Protegido se existe `state.json` e o domínio de teste do modo resolve para `0.0.0.0` / `::` (no Windows, erro 11004). Não olha se o DNS configurado é o conjunto daquele modo. Um Pi-hole, ou outro bloqueador, que zere `doubleclick.net` faz o modo anúncios parecer nosso. A função `cloudflare_dns_active` existe e não entra nessa decisão.

A ADR-022 já manda trocar de modo com a proteção ligada, reaplicando e mantendo o DNS original. A GUI só mostra os rádios em Desprotegido (`linux_gtk.rs`, `windows_winui.rs`). O texto de "Verificar" (`theme.rs`, `verdict`) não diz qual modo passou.

O kit em `Shared/guialar-digital-devops/publish-kit/` (CI Cargo, licenças MIT e Apache, CHANGELOG, nota de migração, ADRs 018–023) não está no repositório.

## Decisão
Polimento local, numa série de commits do gl-refatorador. Sem push, sem tag, sem release, sem `--force`. O e-mail pessoal no `Cargo.toml` e no README não muda.

### 1. Protegido só com o nosso DNS e a sonda
`check_status` devolve Protegido só se as duas coisas valem:

- a sonda do modo no manifesto bloqueia (`probe_host`: família `malware.testcategory.com`, anúncios `doubleclick.net`);
- todo DNS configurado, IPv4 e IPv6, pertence ao conjunto **daquele** modo (`FilterMode::ipv4` / `ipv6`), não a "qualquer filtro do GuiaLar".

Senão, com manifesto presente, é Parcial, com frase leiga. Casos:

- sonda bloqueia, mas há DNS de fora do conjunto: outro filtro ou uma troca no meio do caminho;
- o conjunto é o nosso, mas a sonda não bloqueia: a frase que já existe (filtro não respondeu);
- sem manifesto: continua Desprotegido, mesmo que o DNS ainda seja nosso.

Não ler só `/etc/resolv.conf`. No Linux com NetworkManager o arquivo pode mostrar `127.0.0.53`. Usar o mesmo lugar que `cloudflare_dns_active` já usa (resolv.conf e, se houver, o perfil ativo do `nmcli`). Stub local sozinho não conta como DNS de fora nem como nosso.

### 2. "Verificar" diz o modo
Em Protegido, `verdict` usa a mesma distinção de `description`: família bloqueia sites perigosos e adultos; anúncios bloqueia anúncios e rastreadores. Uma frase só. Linux e Windows leem os dois de `theme.rs`.

### 3. A GUI troca de modo com a proteção ligada
Os mesmos rádios aparecem em Protegido e Parcial, não em Trabalhando. Mudar o rádio chama o `switch_mode` que já existe: uma elevação, DNS original do manifesto preservado. Anúncios não mexe em navegador. Família reinstala a política. Continua uma ação primária. Desprotegido segue como está.

### 4. O kit entra no repo, e para aí
O gl-refatorador copia o publish-kit para o `guialar-rs` e commita, autoria dele. Inclui esta ADR em `docs/adr/`. O README passa a citar MIT e Apache, porque os dois arquivos passam a existir. Não editar o e-mail. O gl-devops não commita e não envia nada ao GitHub.

## Fora desta auditoria
Push na main, tag `java-final`, tag `v0.3.0`, release. Assinatura do `.exe`. Enter e Espaço no GTK. Trava por PID no Windows. DNS de anúncios da Mullvad. Polkit gráfico, Narrador, contraste, cargo-dist. Atividades acadêmicas.

## Ordem
1. gl-refatorador: itens 1 a 4, testes novos da tabela de `check_status` (sem rede: manifesto + DNS simulado; os casos de rede ficam na VM).
2. gl-reviewer: só o commit novo. Olhar a conjunção da sonda com o conjunto do modo, a troca pela GUI, e se o diff não altera o e-mail nem traz caminho de máquina pessoal.
3. gl-qa, nas 3 VMs, no binário desse commit: família ativa → 🟢; DNS trocado à mão para `1.1.1.1` → 🟡; anúncios → 🟢; anúncios → desativar → `10.0.2.3`; manifesto apagado com AdGuard ativo → desativar volta o DNS de antes; clique nos rádios com a proteção ligada (família ↔ anúncios) e em "Abrir site"; no Windows, anúncios → família deixa política no registro, família → anúncios tira. Trava órfã em fedora e win11.
4. gl-usuario: uma sessão leiga na void, depois que o qa disser que o binário está lá. Errar de propósito, perguntar no canal. Não corrige e não lê código.

## Consequências
O gate 3 da ADR-023 fecha no `3bc3b0b` e reabre neste commit, até a revisão nova. Os gates 2 e 7 continuam abertos: a CI só fica verde no GitHub depois do push, e o push espera o "pode subir".

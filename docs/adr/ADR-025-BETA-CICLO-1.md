# ADR-025 — Beta, ciclo 1: ligar, provar, desfazer

Status: Aceita para o trabalho local (2026-11-10) · Autor: gl-arquiteto · Pedido: beta usável (Dr. Eggbot, seq 1472)

O Scope Writer ainda vai escrever o escopo da beta. Até o Guilherme aprovar esse rascunho, o ciclo 1 fica neste mínimo. Não amplia o produto.

## Decisão
O ciclo 1 só fecha quando, em Linux e Windows, uma pessoa consegue:

1. ligar a proteção familiar ou a de só anúncios;
2. ver na hora se **aquela** proteção está valendo;
3. desfazer e voltar ao DNS de antes.

Sem push, sem tag, sem release no GitHub. Sem telemetria, sem histórico de navegação, sem certificado, sem VPN que intercepta tráfego, sem modelo opaco. Os resolvedores continuam os mesmos: família `1.1.1.3` / `1.0.0.3`, anúncios `94.140.14.14` / `94.140.15.15`. A escolha é regra explícita, não medição de quem bloqueia mais.

A ADR-024 continua valendo (DNS do modo + sonda, frase do Verificar com o modo, rádios com a proteção ligada, kit no repo, e-mail intocado). Este texto só acrescenta o que a auditoria achou depois.

### Sonda da família
`malware.testcategory.com` também é bloqueado pelo DNS da Cloudflare que só pega malware (`1.1.1.2`). Trocar um dígito de `1.1.1.3` para `1.1.1.2` deixaria a tela dizer família ativa sem bloquear conteúdo adulto. A sonda da família passa a ser `nudity.testcategory.com`. Anúncios continua `doubleclick.net`. Medição do gl-reviewer em 2026-11-10, nos próprios resolvedores. A página do fabricante fica com o Research Desk; se a página contradisser o teste, a sonda muda de novo.

### Sem DNS de fuga
Em `apply_systemd_resolved` o drop-in manda `FallbackDNS=1.1.1.1 8.8.8.8`. Se o filtro falha, o sistema cai num DNS sem filtro e a proteção some em silêncio. Proibido qualquer resolvedor fora do conjunto do modo escolhido. Tirar a linha `FallbackDNS`. Não substituir por outro DNS público.

### Arquivo temporário previsível
`/tmp/99-guialar.conf.tmp` é gravado e depois copiado com root (`linux.rs`, `apply_systemd_resolved`). O mesmo caminho que o runit e o `resolv.conf` já evitam, escrevendo pelo stdin. Este caminho faz igual. Sem arquivo em `/tmp`.

### Desativar no Windows é ato explícito
A caixa de confirmação fala em "sites perigosos e adultos" mesmo no modo anúncios. O texto segue o modo. O botão padrão é Não, para um Enter não desfazer.

### WIP do devops entra no commit
Há 5 arquivos modificados e não commitados em cima de `6afcd2f`: `ci.yml`, `release.yml`, `package-appimage.sh`, e o caminho de máquina removido das ADRs 018 e 019. O gl-refatorador revisa e commita, autoria dele. Não descartar.

## Fora
Push, tag `java-final`, tag `v0.3.0`, release pública, assinatura do `.exe`, trava por PID no Windows, DNS da Mullvad, Enter/Espaço no GTK (só reabre se o qa reproduzir, nesta build, desativar sem querer), tela pública, atividades acadêmicas.

## Ordem
gl-refatorador implementa e commita. gl-reviewer e Change Reviewer leem esse commit. gl-qa testa nas 3 VMs. gl-usuario usa a void como leigo. Design Critic olha a tela dessa build. gl-devops empacota AppImage e exe locais, sem publicar. Eu registro o ciclo de novo em `Shared/guialar-digital-beta/STATUS.md`.

## Sinal para o Guilherme
Isto não troca a fundamentação (resolvedores, regras explícitas, sem coleta). Muda como a gente prova que o filtro pegou, e tira um DNS sem filtro que entrava quando o nosso falhava.

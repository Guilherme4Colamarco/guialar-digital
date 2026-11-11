# ADR-026 — Plano antes de alterar, e diagnóstico local

Status: Aceita (2026-11-10) · Autor: gl-arquiteto · Guilherme aprovou "mínimo mais P2 e P3" no canal.

P1 (indicação de navegador como trabalho novo) não entrou. Os links que já existem ficam.

## Decisão
### P2. Confirmar antes de mudar o DNS
Na janela, antes de aplicar e antes de trocar de modo com a proteção ligada, a pessoa vê o DNS de agora e o DNS que vai entrar, e confirma. Sem confirmação, nada muda.

A linha de comando não ganha essa pergunta. Script que chama `apply` continua direto.

Desativar no Linux passa a pedir confirmação, como o Windows já pede. O texto segue o modo. O padrão é não desfazer.

### P3. Diagnóstico local, sem enviar nada
A tela de detalhes mostra três coisas que o programa já consegue ler: o gerenciador de rede, os DNS configurados, e uma frase fixa de que não dá para ver daqui se o navegador usa DNS próprio (DoH). Não há sonda nova, não há leitura de histórico, não há telemetria.

Não inventar um detector de DoH. Dizer o limite é o requisito. Afirmar que "não há DoH" seria mentira.

## Fora
Motor que cruza diagnóstico, perfil e teste. Medir eficácia. Push, tag, release.

## Ordem
O pacote do `87c9c0e` segue para as VMs como está. P2 e P3 são o commit seguinte, autoria gl-refatorador, sem push. O qa testa o `87c9c0e` primeiro. P2 e P3 entram no roteiro só depois desse commit.

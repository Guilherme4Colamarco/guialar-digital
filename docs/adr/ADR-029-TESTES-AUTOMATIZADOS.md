# ADR-029 — Testes automatizados (pirâmide e gate)

Data: 2026-11-11 · Status: aceita · Autor: gl-arquiteto · Pedido: Guilherme

## Contexto
Hoje há ~87 testes unitários (`#[test]` em verify, config, policy, theme, sysutil, integration, win32, platform), 9 de contrato da CLI (`tests/cli_contract.rs`) e `ci.yml` com fmt + clippy + test nos alvos linux-gnu, linux-musl e windows.
O que NÃO tem teste automático: o que realmente falhou nesta beta. `apply`/`revert` (nmcli, systemd-resolved, resolv.conf, PowerShell) só foi exercitado por clique manual nas VMs. O defeito do Windows (UAC negado, nada aplicado) só apareceu depois de ~10 rodadas de GUI.
Regra do projeto (memória): binários e testes do GuiaLar rodam só em VM isolada (win11, fedora, void) ou no CI do GitHub, nunca no host PikaOS.

## Decisão
Pirâmide em 4 camadas. Camadas 1–3 rodam em `cargo test` (CI + VM). Camada 4 roda em VM e em job E2E do CI.

1. **Unitários puros** (rápidos, sem I/O). Preencher lacunas: `parse_dns_ips`, `plan_apply` (tabela de decisão: sem manifesto / DNS do modo / DNS de fora / ilegível), migração de manifesto antigo, mescla do `policies.json` do Firefox, textos de estado → botões (modelo de visão em `ui/theme.rs`, sem GTK/WinUI).
2. **Costura de comandos.** `trait CommandRunner { fn run(&self, prog, args) -> Output }` na camada `platform/`. Produção usa `RealRunner`; testes usam `FakeRunner` que grava argv e simula estado de DNS. Sem mudar comportamento: mesmo argv, mesma ordem. Testes: (a) argv exato de apply/revert por backend; (b) propriedade `apply ∘ revert = estado original` no Fake; (c) apply duas vezes é idempotente; (d) revert sem manifesto não falha e não altera; (e) falha do comando (UAC negado = exit != 0) deixa manifesto e DNS intactos e devolve a mensagem «Nada foi alterado».
3. **Integração com diretório de estado isolado** (`HOME`/`LOCALAPPDATA` temporários): manifesto grava, relê, corrompido vira Parcial, remove no revert. Já existe parcialmente em `verify.rs`.
4. **E2E headless pela CLI, em VM real** (sem GUI, sem clique): `scripts/e2e.sh` (Linux) e `scripts/e2e.ps1` (Windows). Roteiro fixo: `status` → `apply` modo família → ler DNS pelo SO (`resolvectl`/`ipconfig`) → sonda `nudity.testcategory.com` = `0.0.0.0` → `apply` de novo (idempotente) → `revert` → DNS igual ao de antes, `state.json` ausente, sonda volta a resolver. Saída: um `PASS/FAIL` por passo, exit != 0 se algum falhar. Windows: precisa de shell elevado; se o SSH do `claude` não vier elevado, o script se recusa e diz isso, não finge.

**GUI não é automatizada por pixel.** O que a GUI decide vai para a camada 1 (modelo de visão). Smoke manual de GUI fica só para o candidato a release (um ciclo, não dez).

## Gates
- Merge/push em branch: `fmt`, `clippy -D warnings`, `cargo test --locked` nos 3 alvos (já existe).
- Release candidate: camada 4 verde em fedora e win11 (void quando houver login), relatório em `RESULTADO-QA-<sha>.md`.
- Job `e2e` no `ci.yml` (`workflow_dispatch` + noturno), não bloqueia PR: runner Windows já é admin; Linux roda no modo `resolv.conf` direto em container privilegiado. Se a rede do runner for instável, vira informativo, não gate.
- Cobertura: `cargo llvm-cov` informativo, sem número mínimo (não vira meta vazia).

## Regra de regressão
Todo hotfix entra com um teste que falha antes e passa depois, no commit do próprio hotfix. Sem teste, o reviewer devolve.

## Fora de escopo
Fuzzing, testes de carga, automação de GUI por captura de tela, mock do GitHub.

## Limites
- **UAC negado no Windows não passa pela costura.** `run_elevated_hidden` (ShellExecuteEx) fica fora do `CommandRunner`; os testes só provam a tradução do resultado (`elevation_result`) e a mensagem «Nada foi alterado». Que o UAC negado deixa manifesto e DNS intactos no Windows real é coberto pelo E2E do gl-qa (`scripts/e2e.ps1`) e pela checagem manual do release candidate. No Linux o critério (e) é coberto pela costura (pkexec 126).
- Testes só gravam estado em `testenv::Sandbox`; em `cargo test`, manifesto e trava de operação falham alto fora dele.

## Divisão
- gl-refatorador: camadas 1–3 (inclui a costura `CommandRunner`; diff mínimo, sem mudar comportamento).
- gl-qa: camada 4 (scripts + execução nas VMs + relatório).
- gl-devops: job `e2e` e relatório de testes no `ci.yml`.
- gl-reviewer: revisa a costura (garantir que o argv de produção não mudou).

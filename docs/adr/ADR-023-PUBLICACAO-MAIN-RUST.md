# ADR-023 — Publicar guialar-rs na main, substituindo o Java

Status: Aceita (2026-11-09) · Autor: gl-arquiteto · Pedido: Guilherme (seq 1319)

## Decisão
- `main` passa a ser o Rust. O Java fica preservado na branch `legacy` (já em d7e08cb) e numa tag `java-final`. Nada é apagado.
- A troca é feita por um commit/merge normal, sem `--force`. O histórico Java continua acessível.
- Ordem: ADR-022 implementada → reviewer → correções → QA (3 VMs, uso normal + "todo errado" + modos family/ads) → devops publica.

## Gates para publicar (todos obrigatórios)
1. `cargo fmt --check`, `clippy -D warnings`, `cargo test` verdes em Linux gnu, musl e Windows.
2. CI do GitHub trocada de Ant para Cargo (fmt, clippy, test, build Linux + Windows); a CI verde no PR é o portão.
3. Revisão do gl-reviewer sem achados ALTO/CRÍTICO em aberto.
4. QA PASS nas 3 VMs, incluindo apply ads → revert → DNS original e fallback com AdGuard ativo.
5. Repositório limpo: sem `target/`, binários, logs, caminhos `/home/...` ou dados pessoais; `.gitignore` e `Cargo.lock` corretos.
6. README em PT-BR leigo (o que faz, instalar, desfazer, modos, navegadores recomendados, limitações), LICENSE-MIT + LICENSE-APACHE, CHANGELOG 0.3.0, as ADRs em `docs/adr/` e uma nota de migração do Java.
7. Release `v0.3.0` com AppImage + .exe + SHA256SUMS, gerada pela CI (não binário feito à mão).

## Pendências do usuário (não bloqueiam o polimento, bloqueiam o push)
- Autoria dos commits: DECIDIDO — manter "gl-refatorador" (usuário, 2026-11-09). Não reescrever histórico.
- Aprovação final explícita antes do push na main (ação pública).

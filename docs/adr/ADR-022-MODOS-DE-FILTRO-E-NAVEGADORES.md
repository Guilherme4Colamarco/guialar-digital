# ADR-022 — Modos de filtro DNS e recomendação de navegadores privados

Status: Aceita (2026-11-09) · Autor: gl-arquiteto · Pedido: Guilherme (seq 1312)

## Contexto
Hoje há um único modo: Cloudflare for Families (1.1.1.3 / 1.0.0.3), que bloqueia malware e conteúdo adulto.
O usuário quer (a) um modo que bloqueie só anúncios e rastreadores, e (b) recomendações de navegador.

"uBlock Origin DNS" não existe: uBlock Origin é uma extensão de navegador, não um servidor DNS. O equivalente em DNS é o AdGuard DNS Default.

## Decisão
### 1. Modo de filtro (enum `FilterMode`)
| Modo | Rótulo na UI | IPv4 | IPv6 |
|---|---|---|---|
| `Family` (padrão atual) | "Proteção familiar" | 1.1.1.3, 1.0.0.3 | 2606:4700:4700::1113, ::1003 |
| `AdsOnly` | "Só bloquear anúncios" | 94.140.14.14, 94.140.15.15 | 2a10:50c0::ad1:ff, 2a10:50c0::ad2:ff |

- Os IPs ficam numa tabela `const` em `core`. Não misturar pares de modos diferentes.
- CLI: `guialar apply [--mode family|ads]`, com padrão `family` (contrato atual preservado).
- O manifesto ganha `"mode"`, com `#[serde(default)]` = `family` para manifestos antigos.
- `sanitize_original_dns` e a detecção `filter_dns_active` / `revert_dns_fallback` reconhecem os **dois** conjuntos de IPs. Isso é obrigatório: sem isso, o fallback deixa o usuário preso no AdGuard.
- `check_status` reporta o modo ativo. Trocar de modo com a proteção ligada = reaplicar mantendo o DNS original do manifesto (nunca salvar o IP de um filtro como original).
- Políticas de navegador (SafeSearch etc.) só no modo `Family`; o modo `AdsOnly` não toca nos navegadores.
- GUI (ADR-020): escolha via 1 grupo de rádio/`AdwComboRow` acima do botão primário. Continua com 1 ação primária. IPs só em Detalhes.

### 2. Recomendação de navegadores (somente informativa)
Página/seção "Navegadores recomendados" com links oficiais. **Não instala nem altera nada.**

| Perfil | Navegador | Nota curta |
|---|---|---|
| Casual | Brave | Bloqueio de anúncios embutido, parecido com o Chrome |
| Minimalista | Helium | Chromium enxuto, uBlock Origin já incluído |
| Diferente/focado | Zen Browser + uBlock Origin | Baseado no Firefox, abas verticais |
| Extra (opcional) | Mullvad Browser | Anti-fingerprint, sem conta |
| Extra (opcional) | LibreWolf | Firefox endurecido, uBlock Origin incluído |

O refatorador confere os links oficiais antes de gravar. Os textos ficam em PT-BR leigo.

## Consequências
- Testes: tabela de IPs, serde do `mode` (manifesto antigo → `family`), fallback com AdGuard ativo, sanitize para os dois conjuntos e contrato CLI `--mode`.
- QA nas VMs: apply ads → DNS 94.140.x → revert → 10.0.2.3; troca family↔ads; manifesto apagado com AdGuard ativo → fallback.

## Fontes
- https://adguard-dns.io/public-dns.html (IPs do AdGuard DNS Default)

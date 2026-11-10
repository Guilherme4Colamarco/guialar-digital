# Changelog

## 0.3.0 — 2026-11-09

Primeira versão pública em Rust. Substitui o Java na `main`. O Java permanece na branch `legacy` e na tag `java-final`.

### Proteção
- Dois modos de filtro DNS: **Proteção familiar** (Cloudflare for Families `1.1.1.3` / `1.0.0.3`) e **Só bloquear anúncios** (AdGuard DNS Padrão `94.140.14.14` / `94.140.15.15`).
- Desativar devolve o DNS original. Se o manifesto (`state.json`) sumir ou corromper, o fallback volta a placa para DHCP.
- Trava de operação (`OpLock`) com PID; trava órfã não prende o programa.
- Instância única da janela. Subcomandos internos exigem administrador.

### Interface
- Linux: GTK 4 + Libadwaita (carregados em tempo de execução).
- Windows: Win32 / Fluent. Executável GUI (sem console no duplo clique).
- Uma ação primária e uma secundária por estado (Desprotegido, Protegido, Parcial, Trabalhando).
- Seção informativa de navegadores recomendados (Brave, Helium, Zen + uBlock Origin, Mullvad Browser, LibreWolf). Não instala nada.

### Distribuição
- Linux: AppImage glibc (GTK do sistema via `dlopen`). Integração no menu no primeiro uso (`ADR-021`).
- Windows: `guialar.exe` único, ainda sem assinatura digital.

### CLI
```
guialar status
guialar apply [--mode family|ads]
guialar revert
guialar browser
```

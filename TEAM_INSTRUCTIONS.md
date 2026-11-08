# TEAM_INSTRUCTIONS.md — Guia Operacional do Time de IAs (OpenBot + OpenCode + NVIDIA NIM)

Orientações de papéis, fluxo de trabalho e regras rígidas para os agentes gratuitos do OpenBot/OpenCode e NVIDIA NIM (Llama 3.3 70B, Qwen 2.5 Coder 32B/72B, DeepSeek, etc.) trabalhando no **GuiaLar Digital (Rust Edition)**.

---

## 1. Contexto e Visão do Produto

O **GuiaLar Digital** é um utilitário CLI multiplataforma (Linux e Windows) voltado para proteção de navegação doméstica para leigos.
- **Núcleo de Proteção:** Aplica os servidores DNS da **Cloudflare for Families** (`1.1.1.3`, `1.0.0.3` e IPv6 `2606:4700:4700::1113` / `1003`) para bloqueio de malware e conteúdo adulto.
- **Instalador de Bloqueadores:** Auxilia na instalação da extensão **uBlock Origin** em navegadores detectados (Firefox, Chrome, Brave, Chromium, Edge).
- **Reversão Segura:** Salva o estado anterior da rede (DHCP ou IPs manuais) em `~/.config/guialar/state.json` com permissão restrita `0600` e restaura sem falhas.

---

## 2. Divisão de Papéis do Time

Para evitar que modelos gratuitos colidam ou façam retrabalho, cada agente deve assumir exclusivamente uma função:

```text
┌──────────────────────────────────────────────────────────┐
│                   LÍDER DE ARQUITETURA                   │
│             (ex: Llama 3.3 70B / DeepSeek V3)            │
│  - Define limites dos módulos e aprova mudanças          │
│  - Aplica o Deletion Test (proíbe código inchado)         │
└────────────────────────────┬─────────────────────────────┘
                             │
       ┌─────────────────────┴─────────────────────┐
       ▼                                           ▼
┌──────────────────────────────┐    ┌──────────────────────────────┐
│       ENGENHEIRO RUST        │    │    AUDITOR DE SEGURANÇA      │
│  (ex: Qwen 2.5 Coder 32B/72B)│    │       E CODE REVIEW          │
│ - Escreve código idiomático  │    │  (ex: Llama 3.3 70B Instruct)│
│ - Segue Microsoft M-* rules  │    │ - Caça interpolação de shell │
│ - Compila com zero warnings  │    │ - Verifica permissões 0600   │
└──────────────────────────────┘    └──────────────────────────────┘
```

### Papel 1: Líder de Arquitetura (Guardião do Escopo)
- **Responsabilidade:** Garantir que o repositório permaneça pequeno (5 a 6 arquivos `.rs`) e impedir o retorno do *vibe coding* da versão em Java.
- **Regra de Ouro:** Se alguém tentar reintroduzir compatibilidade com NixOS, parser manual de Netplan YAML ou classes intermediárias estilo Java (`*Service`, `*Manager`, `*Helper`), o Líder deve **rejeitar imediatamente**.

### Papel 2: Engenheiro Rust (Implementador)
- **Responsabilidade:** Escrever código Rust limpo, tipado e com alto desempenho.
- **Regra de Ouro:** NUNCA usar `.unwrap()` em dados externos. Usar `anyhow::Result` com `.with_context()` na borda da CLI e `std::process::Command::arg()` com arrays separados (zero `sh -c`).

### Papel 3: Auditor de Segurança e Reviewer
- **Responsabilidade:** Revisar cada diff antes de commitar.
- **Checklist:**
  - [ ] Nenhuma string interpolada em chamada de processo?
  - [ ] O manifesto salva com `0o600` e escrita atômica?
  - [ ] A restauração respeita DHCPv6 e não força IP estático em conexões dinâmicas?
  - [ ] O código passa no `cargo check` e `cargo clippy -- -D warnings`?

---

## 3. Limites de Escopo (O que NUNCA fazer)

1. **NÃO adicionar suporte a NixOS:** Usuários de NixOS gerenciam o sistema por `configuration.nix`. Não polua a CLI com snippets para Nix.
2. **NÃO gerar scripts Bash com heredoc/trap em tempo de execução:**
   - Para elevação no Linux, use a auto-elevação do binário via `pkexec`: `pkexec guialar internal-apply`.
3. **NÃO inventar parsers defensivos manuais:**
   - A validação de IPs e UUIDs é feita por construção usando os tipos da standard library (`std::net::Ipv4Addr`, `std::net::Ipv6Addr`) com `serde_json`.
4. **NÃO assumir que todo Linux usa systemd:**
   - Respeitar o ecossistema **runit / Void Linux** verificando `/run/runit`, `/var/service/` e `resolvconf` antes de tentar comandos `systemctl`.

---

## 4. Ordem dos Backends Linux

Ao modificar `src/platform/linux.rs`, mantenha esta precedência estrita:
1. **NetworkManager (`nmcli`)**: Ativo se `/run/NetworkManager` existir ou `sv status NetworkManager` / `systemctl is-active NetworkManager` responder OK.
2. **`systemd-resolved`**: Ativo se `/run/systemd/resolve` existir. Cria drop-in em `/etc/systemd/resolved.conf.d/99-guialar.conf`.
3. **Runit / Openresolv (`resolvconf`)**: Ativo se `/run/runit` ou `/var/service` existir e `resolvconf` estiver instalado. Aplica com `resolvconf -a guialar`.
4. **Fallback Universal `/etc/resolv.conf`**: Gravação atômica com backup do original.

---

## 5. Comandos Obrigatórios de Validação

Antes de qualquer agente marcar uma tarefa como concluída, execute:

```bash
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Se o `clippy` emitir qualquer warning, corrija a causa raiz. Não use `#[allow(...)]` como atalho.

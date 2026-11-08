# AGENTS.md — Guia Universal para Agentes de IA

Instruções fundamentais para qualquer agente autônomo trabalhando no repositório **GuiaLar Digital (Rust Edition)**.

1. **Objetivo:** Utilitário CLI para proteção de rede doméstica (Cloudflare Family 1.1.1.3 / 1.0.0.3) e instalador de uBlock Origin.
2. **Arquitetura:** 5 deep modules concisos. Proibido adicionar NixOS ou Netplan YAML manual.
3. **Plataformas:** Linux (NetworkManager -> systemd-resolved -> runit/resolvconf -> /etc/resolv.conf) e Windows.
4. **Segurança:** Zero concatenação de strings em chamadas de shell. Usar tipos nativos IpAddr com serde_json.
5. **Comandos de Validação:** `cargo check`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt -- --check`.

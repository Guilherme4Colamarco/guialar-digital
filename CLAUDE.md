# CLAUDE.md — Guia Claude Code

- Comandos: `cargo check`, `cargo test`, `cargo clippy -- -D warnings`.
- Estilo: Microsoft Pragmatic Rust (M-* rules). Sem .unwrap() em dados externos.
- CLI: clap derive, saída limpa em stdout, diagnósticos em stderr, ExitCode em main().
- Manifesto: ~/.config/guialar/state.json salvo atomicamente com 0600.

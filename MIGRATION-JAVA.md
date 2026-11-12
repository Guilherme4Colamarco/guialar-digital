# Migração do Java para o Rust

A `main` deste repositório passou a ser o GuiaLar em Rust (`guialar` 0.3.0).

O programa Java (Ant, JUnit, JAR) **não foi apagado**. Ele está na branch `legacy` e, a partir da publicação, na tag `java-final` (commit `d7e08cb`; a tag **será criada** junto com a publicação e ainda não existe no repositório).

```bash
git checkout java-final    # árvore Java congelada (depois que a tag for criada)
git checkout legacy        # mesma ponta, branch permanente (já existe)
```

## O que muda para quem usava o JAR

| Antes (Java) | Agora (Rust) |
|--------------|--------------|
| `java -jar guialar-digital.jar` | `guialar-<versão>-x86_64.AppImage` no Linux ou `guialar-<versão>-windows.exe` no Windows |
| Pacotes `.deb` / `.rpm` / JAR | AppImage + exe (release da CI, com `SHA256SUMS`) |
| Um filtro DNS (Cloudflare Family) | Family ou só anúncios (AdGuard) |
| Swing | GTK 4 / Libadwaita (Linux), Win32 (Windows) |

O manifesto de estado não é o mesmo arquivo do Java. Quem tinha proteção ligada no Java deve desativar pelo JAR da `legacy` (ou da tag `java-final`, quando existir) **antes** de usar o binário Rust, ou conferir o DNS da placa depois.

## CI

Os workflows Ant (`build-and-test.yml`, `quick-check.yml`) saem da `main` com o Java. A `main` Rust usa Cargo: formato, clippy, testes e build Linux + Windows.

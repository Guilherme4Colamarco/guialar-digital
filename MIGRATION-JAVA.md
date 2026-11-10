# Migração do Java para o Rust

A `main` deste repositório passou a ser o GuiaLar em Rust (`guialar` 0.3.0).

O programa Java (Ant, JUnit, JAR) **não foi apagado**. Ele está na branch `legacy` e na tag `java-final` (commit `d7e08cb`).

```bash
git checkout java-final    # árvore Java congelada
git checkout legacy        # mesma ponta, branch permanente
```

## O que muda para quem usava o JAR

| Antes (Java) | Agora (Rust) |
|--------------|--------------|
| `java -jar guialar-digital.jar` | AppImage no Linux ou `guialar.exe` no Windows |
| Pacotes `.deb` / `.rpm` / JAR | AppImage + exe (release da CI) |
| Um filtro DNS (Cloudflare Family) | Family ou só anúncios (AdGuard) |
| Swing | GTK 4 / Libadwaita (Linux), Win32 (Windows) |

O manifesto de estado não é o mesmo arquivo do Java. Quem tinha proteção ligada no Java deve desativar pelo JAR da tag `java-final` **antes** de usar o binário Rust, ou conferir o DNS da placa depois.

## CI

Os workflows Ant (`build-and-test.yml`, `quick-check.yml`) saem da `main` com o Java. A `main` Rust usa Cargo: formato, clippy, testes e build Linux + Windows.

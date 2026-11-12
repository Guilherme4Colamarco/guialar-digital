# Como validar o release.yml sem publicar nada

Issue #20. Base: branch `rust` em `95be2b1`. Arquivo analisado: `.github/workflows/release.yml`.

## Resposta curta

Dê ao `release.yml` um gatilho `push` numa branch descartável (ex.: `release-dry-run`) e
condicione o passo de release a `github.ref_type == 'tag'`. O run de ensaio compila os três
artefatos (AppImage, `.exe`, `SHA256SUMS`) e os sobe só como artefatos do run. Não cria tag
nem release. `workflow_dispatch` sozinho **não serve hoje**, e o motivo está abaixo.

## O que cada opção resolve

| Opção | Funciona hoje? | Observação |
|---|---|---|
| `workflow_dispatch` | Não | Só dispara se o arquivo do workflow existir na branch padrão [1]. A `main` é Java e não tem `release.yml` (tem só `build-and-test.yml`, `quick-check.yml` e um README). Vale manter para depois que a `main` virar Rust. |
| `push` numa branch | **Sim** | Eventos `push` usam o workflow do commit enviado, "incluindo workflows que não estão na branch padrão" [1]. É o caminho recomendado. |
| `pull_request` para `main` | Sim | Roda na ref `refs/pull/N/merge` [1]. Serve, mas exige abrir um PR `rust` → `main`, que é exatamente o passo que a ADR-023 quer controlar. |
| Release em rascunho (`draft: true`) | Arriscado | A action só aceita rodar fora de tag quando `draft` ou `tag_name` estão definidos [3]. Sem `tag_name`, ela usa `github.ref_name` [2], ou seja, um rascunho com tag `release-dry-run`. Rascunhos só aparecem para quem tem push [4], mas a documentação não diz se a tag é criada ao salvar o rascunho [4][5]. Só use se precisar ensaiar o upload para Releases, e apague o rascunho depois. |
| `act` local | Parcial | Imagens Docker só existem para Ubuntu. Windows exige `-P windows-latest=-self-hosted` num host Windows [6]. O job `publish` usaria um token real. Serve como checagem rápida do job Linux, não como prova do Gate 7. |

## Riscos concretos no release.yml atual

1. **O release quebra fora de tag.** `softprops/action-gh-release@v2` sem `draft` nem `tag_name`
   lança `⚠️ GitHub Releases requires a tag` quando a ref não é `refs/tags/` [3]. Qualquer gatilho
   extra exige um `if: github.ref_type == 'tag'` nesse passo.
2. **A versão vem do nome da ref.** `VERSION: ${{ github.ref_name }}`. Num ensaio em branch, o
   nome vira `guialar-release-dry-run-x86_64.AppImage`. Numa tag errada (ex.: `v0.3.1` com
   `Cargo.toml` em `0.3.0`), nada detecta a divergência. Leia a versão do `Cargo.toml` e, em
   tag, falhe se `tag != v<versão>`.
3. **O código nunca passou pela CI.** O `ci.yml` só escuta `main`, e a API devolve 404 para
   `release.yml` porque ele nunca foi registrado. O release não roda `fmt`, `clippy` nem `test`.
   A primeira vez que esse código compilaria no runner seria a própria tag.
4. **`llvm-windres` no Windows não foi verificado.** O `build.rs` entra em pânico se
   `llvm-windres` (ou `GUIALAR_WINDRES`) não estiver no PATH. A imagem `windows-latest`
   (Server 2025) tem o LLVM 20.1.8 instalado [7], mas não confirmei que `llvm-windres` está no
   PATH. É o primeiro ponto a conferir no log do ensaio.
5. **A permissão é ampla demais.** `permissions: contents: write` está no nível do workflow e vale
   também para os jobs de build. Use `contents: read` no topo e `write` só em `publish` [8].
6. **O padrão `v*` pega pré-releases.** Uma tag `v0.3.0-rc1` geraria um release público
   "estável". Passe `prerelease: ${{ contains(github.ref_name, '-') }}` ou restrinja o padrão.
7. **O `glibc` do AppImage é recente.** O build roda em `ubuntu-latest` (24.04) [7], então o binário
   exige o `glibc` dessa versão ou mais novo e pode não abrir em distros mais antigas. Ou isso é
   aceito e documentado, ou o build passa a usar uma imagem mais antiga.
8. **A tag precisa apontar para um commit Rust.** Um `push` de tag usa o workflow do commit
   marcado [1]. Uma tag `v0.3.0` num commit da `main` Java não roda nada.
9. **Os toolchains e as actions são flutuantes.** `dtolnay/rust-toolchain@stable` e as actions
   seguem tags móveis, então o ensaio e a tag podem compilar com versões diferentes. Considere
   fixar a versão do Rust (`rust-toolchain.toml`) e fixar as actions por SHA.

## Mudanças sugeridas no workflow (não aplicadas aqui)

```yaml
on:
  push:
    tags: ["v*"]
    branches: [release-dry-run]
  workflow_dispatch:          # útil só quando o arquivo estiver na main

permissions:
  contents: read

# linux/windows: VERSION a partir do Cargo.toml; em tag, comparar com github.ref_name.

  publish:
    permissions:
      contents: write
    steps:
      # ... download-artifact + SHA256SUMS como hoje ...
      - uses: actions/upload-artifact@v4      # sempre: prova do ensaio
        with:
          name: release-bundle
          path: dist/*
          retention-days: 5
      - name: Criar release
        if: github.ref_type == 'tag'
        uses: softprops/action-gh-release@v2
        with:
          files: dist/*
          generate_release_notes: true
          fail_on_unmatched_files: true
          prerelease: ${{ contains(github.ref_name, '-') }}
```

## Roteiro de prova (antes da `v0.3.0`)

1. Aplicar as mudanças acima num PR para a `rust`.
2. `git push origin rust:release-dry-run`. Isso cria só uma branch: nada de tag, nada de release.
3. Critérios de aceite:
   - os 3 jobs ficam verdes;
   - o artefato `release-bundle` contém `guialar-0.3.0-x86_64.AppImage`,
     `guialar-0.3.0-windows.exe` e `SHA256SUMS`;
   - `sha256sum -c SHA256SUMS` passa nos arquivos baixados, sem executá-los;
   - `gh release list` e `git ls-remote --tags origin` não mudaram.
4. Apagar a branch `release-dry-run`.

Os artefatos ficam 90 dias por padrão, e quem tem acesso de leitura pode baixá-los [9]. Num repo
público isso quer dizer qualquer usuário logado. Por isso o `retention-days` curto.

## Fontes

1. GitHub Docs, Events that trigger workflows (`workflow_dispatch`, `push`, `pull_request`): https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows
2. softprops/action-gh-release, README (inputs `draft`, `tag_name`): https://github.com/softprops/action-gh-release#inputs
3. softprops/action-gh-release, `src/run.ts` (checagem de tag): https://github.com/softprops/action-gh-release/blob/master/src/run.ts
4. GitHub REST, Releases ("Only users with push access will receive listings for draft releases"): https://docs.github.com/en/rest/releases/releases
5. GitHub Docs, Managing releases: https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository
6. nektos/act, Runners: https://nektosact.com/usage/runners.html
7. actions/runner-images (README e Windows2025-Readme): https://github.com/actions/runner-images
8. GitHub Docs, Workflow syntax, `permissions`: https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#permissions
9. GitHub Docs, Download workflow artifacts (90 dias, acesso de leitura): https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts

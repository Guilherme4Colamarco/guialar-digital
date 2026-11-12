# Como a CI roda no PR de troca para a main (issue #19)

Pesquisa para o gate 2 da ADR-023 (CI verde no PR). Base: `ci.yml` e `release.yml` da branch `rust` (95be2b1), workflows da `main` (d7e08cb) e docs oficiais do GitHub Actions (consultadas em 2026-11-12).

## Resposta curta

1. **O `ci.yml` já tem o gatilho certo.** Na `rust` ele dispara em `push: [main]`, `pull_request: [main]`, `workflow_dispatch` e `schedule`. O filtro `branches` do `pull_request` é aplicado à branch **base** do PR. Então um PR `troca-rust -> main` dispara o CI sem mudança no `on:` [1].
2. **O workflow que roda vem do PR, não da `main`.** No `pull_request`, `GITHUB_REF` é `refs/pull/N/merge` e o `actions/checkout` pega o commit de merge (PR mesclado na base). Os arquivos de workflow lidos são os desse commit [1]. Ou seja: o `ci.yml` Rust roda no PR mesmo que a `main` ainda não tenha esse arquivo.
3. **Armadilha principal: os workflows Ant da `main` também rodam.** Um merge com `--allow-unrelated-histories` junta as duas árvores. Se `build-and-test.yml`, `quick-check.yml` e `README.md` em `.github/workflows/` (e o código Java/`build.xml`) não forem apagados **no próprio commit de merge**, os workflows "Build and Test" e "Quick Check" também disparam no PR (os dois têm `pull_request: [main]`). Eles podem passar ou falhar, e isso polui o portão. Remova-os no merge (o próprio `ci.yml` diz que ele "substitui os workflows Ant").
4. **Conflitos travam a CI.** "Workflows will not run on pull_request activity if the pull request has a merge conflict" [1]. As únicas colisões add/add entre as árvores são `README.md` e `.gitignore`. Resolva as duas no commit de merge. Como a branch nasce da `main`, o PR não terá conflito enquanto a `main` não andar.
5. **Permissões:** nada a mais. O default do repo para o `GITHUB_TOKEN` já é `read` (API `actions/permissions/workflow`: `default_workflow_permissions: read`). O CI só faz checkout e upload de artefato. Recomendado declarar `permissions: contents: read` no topo do `ci.yml`, para não depender da configuração do repo. Ao declarar uma permissão, as outras ficam `none` [2]. O `release.yml` (`contents: write`, só em tag `v*`) não dispara no PR.
6. **e2e no PR: não.** Ele altera DNS em container `--privileged` (root) e no Windows como admin, depende da rede do runner e está como `continue-on-error`. Por isso não serve de portão. A ADR-029 já diz "não bloqueia PR". Mantenha o `if: schedule || workflow_dispatch`. No PR ele aparece como *skipped*.

## Usuário comum, nunca root

- **Linux:** os runners hospedados rodam como usuário não-root, com `sudo` sem senha [3]. Os jobs `linux-gnu` e `linux-musl` rodam `cargo test` sem `sudo`, então o requisito é atendido. O `sudo` só aparece no `apt-get` do musl. Para tornar isso verificável, adicione antes dos testes um passo de guarda: `test "$(id -u)" -ne 0`.
- **Windows: limitação.** "Windows virtual machines are configured to run as administrators with User Account Control (UAC) disabled" [3]. O `cargo test` do job `windows` roda como administrador (`IsUserAnAdmin() != 0`). Não há opção do runner hospedado para rodar como usuário comum. As saídas são aceitar a limitação e documentá-la na ADR, ou criar um usuário local sem admin no job e rodar os testes com ele (é mais trabalho e mais frágil). Os testes usam `FakeRunner`/costura, então o risco de alterar a VM é baixo, mas o requisito "nunca admin" não vale literalmente no Windows.
- O e2e roda como root/admin de propósito. Mais um motivo para ficar fora do PR.

## Outras armadilhas

- **`pull_request_target`: não usar.** Ele roda no contexto da branch padrão da base, ou seja, usaria os workflows Ant da `main`. A doc o desaconselha quando é preciso buildar ou rodar código do PR [1].
- **`schedule` e `workflow_dispatch` dependem da branch padrão.** O cron "only run[s] on the default branch", e os dois eventos só disparam se o arquivo existir na branch padrão [1]. Hoje a `main` não tem o `ci.yml`. Logo, o e2e noturno e o botão "Run workflow" só passam a existir depois do merge. Para validar o e2e antes, não conte com o dispatch na branch do PR. Depois do merge, rode `gh workflow run CI --ref main`. Em repo público, o cron é desligado após 60 dias sem atividade [1].
- **concurrency (opcional).** Hoje não existe. Pushes seguidos no PR geram runs paralelos (Linux + Windows). Sugestão, cancelando só em PR: `concurrency: { group: "${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}", cancel-in-progress: "${{ github.event_name == 'pull_request' }}" }`. Use `github.workflow` no grupo para não cancelar runs de outros workflows [2].
- **O portão não é técnico.** A `main` não tem proteção de branch (API: "Branch not protected"). "CI verde" só bloqueia se alguém exigir. Opcional: um ruleset na `main` exigindo os checks `Linux gnu (fmt clippy test build)`, `Linux musl (clippy test)` e `Windows (clippy test build)`, mas não o e2e. Se forem exigidos, os nomes dos jobs não podem mudar.
- **`--locked`:** o `Cargo.lock` está versionado na `rust`. Ele precisa sobreviver ao merge.
- **PR aberto por automação:** se o PR for criado ou atualizado com `GITHUB_TOKEN`, os runs exigem aprovação manual [1]. Abra ou atualize o PR com conta de usuário.

## Ajustes mínimos recomendados no `ci.yml` (para quem for mexer, não feito aqui)

1. `permissions: contents: read` no topo.
2. Passo `test "$(id -u)" -ne 0` antes do `cargo test` nos dois jobs Linux.
3. (Opcional) bloco `concurrency` como acima.
4. Nada no `on:`. Nada no job `e2e`.

E no commit de merge: apagar os workflows Ant e o `README.md` de `.github/workflows/`, apagar a árvore Java e resolver `README.md`/`.gitignore`.

## Fontes

1. GitHub Docs, *Events that trigger workflows* (`pull_request`, `pull_request_target`, `schedule`, `workflow_dispatch`): https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows
2. GitHub Docs, *Workflow syntax* (`permissions`, `concurrency`): https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax
3. GitHub Docs, *GitHub-hosted runners* (sudo sem senha no Linux; admin com UAC desligado no Windows): https://docs.github.com/en/actions/reference/runners/github-hosted-runners
4. Repo: `.github/workflows/ci.yml` (rust), `.github/workflows/*.yml` (main), `docs/adr/ADR-023-PUBLICACAO-MAIN-RUST.md`, `docs/adr/ADR-029-TESTES-AUTOMATIZADOS.md`.

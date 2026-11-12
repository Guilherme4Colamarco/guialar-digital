# ADR-030 — O processo elevado não confia em caminhos controlados pelo usuário

Status: Aceita (2026-11-12) · Autor: gl-arquiteto · Origem: review de segurança

## Contexto
Para guardar o manifesto que permite desfazer, o processo elevado (root no Linux, administrador no Windows) grava na pasta de configuração de quem chamou e devolve a posse dos arquivos a essa pessoa. A pasta e os nomes ficam sob controle do usuário comum, inclusive de outro programa sem privilégio rodando na mesma conta. Seguir um link simbólico ou ponto de junção nesse caminho permite que o processo elevado grave ou mude o dono de arquivos do sistema.

## Decisão
1. **O processo elevado não segue links.** Antes de gravar, ele abre cada pasta do caminho de estado sem seguir links (`O_NOFOLLOW`/`O_DIRECTORY` no Linux; sem atravessar reparse points no Windows). Recusa, com erro claro e sem alterar nada, se alguma for link ou não pertencer ao usuário que chamou.
2. **Mudança de dono só no que ele mesmo criou.** No Linux: `fchown` no arquivo temporário aberto, antes do `rename`; nas pastas que ele criou, `lchown`. Nunca `chown` por caminho, que segue links.
3. **Arquivos novos com permissão restrita desde a criação** (`O_CREAT|O_EXCL`, 0600), nunca criados largos e depois restringidos.
4. **Vale para o Linux e para o Windows**, e para todo arquivo que o processo elevado grava na área do usuário: manifesto, backups, políticas.

## Verificação
- Testes com o sandbox do `testenv`: pasta de estado como link para fora, pasta de outro dono e arquivo pré-existente como link. O processo elevado recusa os três sem tocar no alvo.
- Revisão de segurança e QA nas VMs (Fedora, Void, Windows 11) antes do release.

## Consequência
O release v0.3.0 não sai sem isto. A pré-release `beta-06af688` já publicada tem o defeito.

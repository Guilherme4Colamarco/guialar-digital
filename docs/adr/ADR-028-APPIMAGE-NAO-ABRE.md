# ADR-028 — O AppImage tem de abrir a janela, e a elevação não pode sujar o /tmp

Status: Aceita (2026-11-11), revisada em 2026-11-12 com a causa medida · Autor: gl-arquiteto

## Contexto
Numa VM Fedora de teste, o AppImage baixado não abriu a janela por duplo clique: o runtime falhou com `Failed to extract AppImage`. A hipótese inicial era falta de FUSE.

## O que foi medido (2026-11-12)
- FUSE não é a causa. Com FUSE 3 instalado (o runtime do AppImage traz a biblioteca embutida), o mesmo arquivo abre como usuário comum, por execução direta e pelo atalho do menu.
- A falha só reproduz quando o runtime extrai o AppImage em vez de montá-lo, e já existe em `/tmp/appimage_extracted_<md5 do arquivo>` uma pasta `drwx------` de root.
- Quem cria essa pasta é o próprio GuiaLar. Para aplicar ou desfazer, `elevate_self` (`src/platform/linux.rs`) roda o AppImage como root via `sudo`/`pkexec` com `APPIMAGE_EXTRACT_AND_RUN=1`, porque o root não acessa o ponto de montagem FUSE do usuário. O runtime extrai nesse caminho previsível e não apaga a pasta.
- Consequências:
  1. Depois de um apply ou revert, outra extração como usuário comum falha.
  2. Um processo root executa binários de um caminho previsível em um diretório compartilhado. Não testamos exploração, mas a regra é não depender disso.

## Decisão
1. Formato: segue AppImage (ADR-021). Sem deb, rpm ou flatpak. Extrair à mão não é o caminho para quem usa o programa.
2. Elevação com AppImage: a extração do root acontece numa pasta privada, criada pelo próprio root com `mktemp -d` (0700), passada em `TMPDIR`, e apagada ao final, com sucesso ou com erro. O comando elevado continua único (um só pedido de senha) e vale igual para `sudo -n` e `pkexec`.
3. Nada do GuiaLar elevado grava em caminho previsível de `/tmp` compartilhado.
4. O atalho do menu (`.desktop`) aponta sempre para o AppImage (`$APPIMAGE`) quando rodando dele. Rodar o binário de outra pasta (por exemplo, de build) não pode reescrever o atalho do usuário para essa pasta.

## Verificação
- Teste no `FakeRunner`: com `APPIMAGE` definido, o argv elevado cria o diretório privado, exporta `TMPDIR` e o remove; sem `APPIMAGE`, o argv fica como hoje.
- QA na Fedora: apply e revert pela janela e pela CLI, a partir do AppImage. Depois deles, nenhum `/tmp/appimage_extracted_*` sobra, e o AppImage volta a abrir como usuário comum.

## Fora
Trocar o formato do pacote. Instalar FUSE pelo programa.

## Revisão 2 (2026-11-12, depois do review de segurança do primeiro conserto)
O primeiro conserto (pasta privada via `/bin/sh -c` e `TMPDIR`) resolveu a sobra no `/tmp`, mas a revisão achou três problemas:
- A trava de operação morava em `temp_dir()`, então o processo elevado passou a ver outra trava. Apply e revert ficaram sem exclusão mútua.
- O diálogo do polkit passou a mostrar um script de shell, o que assusta e esconde o que vai rodar.
- `APPIMAGE` é herdado por qualquer processo filho de outro AppImage.

Decisão, substituindo o item 2:
1. **Elevar uma cópia do binário, não o AppImage.** O processo do usuário copia `/proc/self/exe` para uma pasta nova e aleatória em `$XDG_RUNTIME_DIR` (0700, do próprio usuário) e eleva essa cópia com `internal-apply`/`internal-revert`. Não há shell, nem extração como root, nem `/tmp`. O polkit mostra o caminho do GuiaLar e o subcomando. O usuário apaga a cópia ao fim, com sucesso ou com erro.
   - Pré-condição medida antes de adotar: a cópia roda os subcomandos internos fora do AppImage, na Fedora e na Void. Se não rodar, para e reporta; não se volta ao shell sem nova decisão.
   - Sem `$XDG_RUNTIME_DIR`, usar `mktemp -d` dentro de uma pasta do usuário que não seja compartilhada. Nunca no `/tmp`.
2. **Trava fora de diretórios temporários:** `/run/guialar/op.lock`, criada pelo processo elevado com `O_NOFOLLOW`. O usuário só lê para checar se está livre.
3. **Só confiar em `APPIMAGE` quando `current_exe` estiver dentro de `APPDIR`**, tanto para o atalho do menu quanto para a elevação.
4. **Código de saída 127 do runtime** aparece como "o AppImage não pôde ser executado", não como "permissão negada".

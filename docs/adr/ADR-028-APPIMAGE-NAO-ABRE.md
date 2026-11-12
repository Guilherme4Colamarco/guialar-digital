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

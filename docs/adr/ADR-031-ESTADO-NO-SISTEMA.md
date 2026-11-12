# ADR-031 — Estado e trava ficam em pasta do sistema, não na área do usuário

Status: Aceita (2026-11-12) · Autor: gl-arquiteto · Substitui a ADR-030 para o estado e a trava · Origem: review de segurança (escrita elevada) e o modo "por cima do ombro" no Windows

## Contexto
O manifesto que permite desfazer, os backups e a trava eram gravados pelo processo elevado na área de quem chamou (`~/.config/guialar`, `%LOCALAPPDATA%`). Isso tem dois problemas:
- O processo elevado escreve num caminho que o usuário comum controla. É a origem da escalada de privilégio tratada na ADR-030.
- No Windows, quando uma conta padrão recebe a senha de um administrador ("por cima do ombro"), o processo elevado roda no perfil do administrador. O estado e a trava vão para outra pasta, e a janela da conta padrão diz que o registro sumiu.

## Decisão
1. **Local do estado** (manifesto e backups), gravado só pelo processo elevado:
   - Linux: `/var/lib/guialar/`, pasta `root:root 0755` e arquivos `0644`. O manifesto só tem endereços de DNS e nomes de conexão, nada de histórico.
   - Windows: `%ProgramData%\GuiaLar\`, com ACL explícita: Administradores e SYSTEM com controle total, Usuários só leitura, sem herança.
2. **Trava:** Linux em `/run/guialar/op.lock`, conforme a ADR-028 rev. 2. Windows em `%ProgramData%\GuiaLar\op.lock`.
3. **O processo do usuário só lê** esses arquivos, para o status e para checar a trava. Ele nunca grava neles.
4. **Na criação**, o processo elevado recusa a pasta, sem alterar nada, se ela já existir com outro dono, outra ACL, como link ou como ponto de reparse.
5. **Migração da beta:** se não houver estado no sistema, mas houver um manifesto antigo na área do usuário, o `revert` o lê uma vez sem seguir links e recusa se for link ou de outro dono. Depois desfaz e apaga o antigo. O `apply` nunca grava mais na área do usuário.
6. **A ADR-030 continua valendo** para o que o processo elevado ainda precisar gravar na área de algum usuário, como o `policies.json` de um Firefox instalado em `%LOCALAPPDATA%`: não segue links nem pontos de reparse e recusa caminho de outro dono.

## Verificação
- Testes com o sandbox do `testenv` apontando a raiz do sistema para uma pasta temporária:
  - estado gravado só lá;
  - status lido sem elevar;
  - pasta pré-existente como link, ou de outro dono, recusada;
  - migração de um manifesto antigo;
  - manifesto antigo como link, recusado.
- QA:
  - Fedora e Void: apply e revert, e `ls -l /var/lib/guialar /run/guialar`.
  - win11: ACL de `%ProgramData%\GuiaLar` e o modo "por cima do ombro" com uma conta padrão.

## Consequência
Os itens 1 a 3 da ADR-030 deixam de ser necessários para o estado: o problema some na raiz. O README passa a dizer onde o estado fica e como apagá-lo.

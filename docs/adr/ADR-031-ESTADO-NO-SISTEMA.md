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

## Emenda 1 (2026-11-12, review do desenho)
- **Windows, criação atômica e protegida:**
  - O caminho vem de `SHGetKnownFolderPath(FOLDERID_ProgramData)`, não da variável de ambiente.
  - A pasta é criada com `CreateDirectoryW` já com a DACL protegida (Administradores e SYSTEM com controle total, Usuários só leitura) e dono Administradores, num passo só. A ACL padrão do `ProgramData` deixa contas comuns criarem pastas lá, e uma pasta criada antes de receber a ACL abre uma janela de ataque.
- **Pasta pré-existente que não é nossa** (outro dono, outra ACL, link ou ponto de reparse): o processo elevado a renomeia para o lado (`GuiaLar.recusada-<data>`), registra isso e recria a correta. Recusar para sempre deixaria a proteção bloqueada por qualquer conta comum. O item 4 fica valendo só para os arquivos dentro dela.
- **Antes de ler `state.json` ou respeitar `op.lock`**, o processo elevado confere que o dono é Administradores ou SYSTEM no Windows, e root no Linux. Se não for, ignora o arquivo e registra.
- **Linux:** `/run/guialar` com `root:root 0755` e `op.lock` com `0644`.
- **Migração do manifesto antigo**, que é do usuário e pode ter sido forjado: antes de agir, o processo elevado valida.
  - Cada conexão ou interface citada tem de existir agora.
  - Cada DNS tem de ser um IP válido.
  - `previous_resolv_conf` só pode ter linhas `nameserver`, `search`, `domain` e `options`, até 4 KB.
  - Se qualquer item falhar, não aplica nada do manifesto: cai no fallback de DNS automático e avisa.
  - A migração sai do código na v0.4.0.

## Emenda 2 (2026-11-12, medida na win11 no 98e2190)
- `SetNamedSecurityInfoW` pelo nome não seguiu junção nem link de pasta na medida. O handle aberto com `OPEN_REPARSE_POINT` fica como defesa em profundidade.
- Limite aceito: um `op.lock` ou `state.json` mantido aberto sem `FILE_SHARE_DELETE` por um processo de outra conta não pode ser apagado. Nesse caso o programa para com erro claro antes de tocar no DNS e se recupera quando o handle fecha. É negação de serviço local, enquanto o processo do intruso viver. A integridade do registro depende do N1 (REVIEW-4cea602): DACL explícita no temp e no op.lock e `is_trusted` conferindo a DACL; o resíduo aceito é apagar o registro, não forjá-lo. Não usar `MOVEFILE_DELAY_UNTIL_REBOOT`. Nomes ou pastas dinâmicos ficam fora da beta.
- Correção da emenda 2 após o N2 (REVIEW-678e502): com a pasta reaberta como resíduo, a conta comum tem FILE_DELETE_CHILD e pode apagar ou renomear qualquer arquivo dela. Por isso o `state.json` é aberto uma vez só (sem FILE_SHARE_DELETE, com OPEN_REPARSE_POINT), conferido pelo handle (dono e DACL) e lido desse mesmo handle. O resíduo aceito na beta passa a ser: apagar, renomear ou reapresentar um registro antigo legítimo do admin; nunca forjar um. Apagar a trava viva permite duas operações elevadas ao mesmo tempo (baixo, aceito). Pós-beta: estado em `HKLM\SOFTWARE\GuiaLar`.
- Nota (REVIEW-62eaa16, B1): como o `state.json` dá leitura a Users, QUALQUER conta comum, sem pré-condição, pode mantê-lo aberto sem FILE_SHARE_DELETE. O efeito continua sendo só negação de serviço: o desfazer devolve o DNS, conclui a reversão das extensões e avisa que o registro não pôde ser apagado; o proteger seguinte para com mensagem clara.

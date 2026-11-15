# ADR-032 — NetworkManager: uma conexão nova não pode ficar sem filtro

Status: **Aceita** (2026-11-15): opção A2, implementada · Origem: revisão de segurança do comportamento (achado S2)

## Contexto
No backend do NetworkManager, o «proteger» grava o DNS do filtro em cada perfil de conexão **ativo naquela hora** (`linux.rs`, `get_active_nm_connections`: só `--active`, só ethernet e Wi-Fi). Um perfil que passa a valer depois (outra rede Wi-Fi, o hotspot de um celular, um perfil novo para a mesma placa) usa o DNS que a rede entregar.

Medido na VM Fedora em 2026-11-15, com o binário da fase 3:

| Momento | DNS do link | Sonda `nudity.testcategory.com` | `guialar status` |
|---|---|---|---|
| Depois de proteger | `1.1.1.3 1.0.0.3` | `0.0.0.0` (bloqueado) | Protegido |
| Depois de ativar um perfil novo na mesma placa | `10.0.2.3` | `104.18.4.35` (abre) | Parcial: «usa outro DNS» |

O programa percebe (mostra «Parcial»), mas só quando alguém abre a janela ou roda `status`: não há aviso. Para uma criança numa conta comum, trocar de rede Wi-Fi não pede senha de administrador nas áreas de trabalho comuns.

O que **não** foi medido: pela sessão SSH da VM o polkit negou a troca de perfil sem `sudo` («Not authorized to control networking»), então o caso «conta comum, sessão gráfica local» não foi exercitado. A brecha em si (perfil novo sem filtro) foi medida com `sudo`.

## Opções

### A. DNS global do NetworkManager (`global-dns-domain-*`) — medida, **não serve**
Gravar `/etc/NetworkManager/conf.d/*.conf` com `[global-dns-domain-*]` e recarregar.

Medido na VM Fedora 43 (NetworkManager 1.54.3 com o plugin `systemd-resolved`):

| Configuração | Efeito |
|---|---|
| Só `[global-dns-domain-*]`, com `reload` e com reinício do serviço | O NetworkManager lê o arquivo (`global-dns-config` no log), mas o link continua em `10.0.2.3` e a sonda abre |
| Com a seção `[global-dns]` explícita e reinício | O link fica **sem DNS nenhum** e nada resolve |

Não fecha a brecha num caso e derruba a resolução no outro. Descartada para sistemas com systemd-resolved; sem resolved não foi medida.

### A2. DNS global do systemd-resolved com `Domains=~.` — medida, **funciona** (recomendada)
Gravar um drop-in em `/etc/systemd/resolved.conf.d/` com `DNS=<servidores do modo>` e `Domains=~.`, e reiniciar o `systemd-resolved`. O domínio de roteamento `~.` faz toda consulta ir para o DNS global, qualquer que seja o DNS do link.

Medido na mesma VM:

| Momento | DNS do link | DNS global | Sonda | `example.com` |
|---|---|---|---|---|
| Antes | `10.0.2.3` | vazio | abre | resolve |
| Com o drop-in | `10.0.2.3` | `1.1.1.3 1.0.0.3` + IPv6 | `0.0.0.0` | resolve |
| Com um perfil novo ativo | `10.0.2.3` | o mesmo | `0.0.0.0` em 5 de 5 | resolve |

- A favor: fecha a brecha onde o Fedora e o Ubuntu de fato resolvem nomes; o programa já tem um backend de systemd-resolved que grava um drop-in; o desfazer é apagar um arquivo; não depende de perfil, então cobre Wi-Fi novo, hotspot, gsm e bridge.
- Contra: em máquinas com NetworkManager **e** systemd-resolved o backend escolhido passa a ser o do resolved, não o do NetworkManager, o que muda a detecção, o registro e a leitura do estado (hoje «Protegido» exige que o DNS do link seja o do filtro; aqui o link continua com o DNS da rede). Nomes internos de uma VPN de trabalho deixam de resolver. Quem já está protegido pelo backend do NetworkManager precisa de um caminho de troca.
- Não coberto: NetworkManager **sem** systemd-resolved (o NetworkManager escreve o `resolv.conf`). Ali a brecha continua e precisa de outra medição.

### B. Script no dispatcher do NetworkManager
Instalar um script em `/etc/NetworkManager/dispatcher.d/` que reaplica o DNS do filtro a cada conexão que sobe.
- A favor: mantém o desenho atual por perfil; serve também sem systemd-resolved.
- Contra: um script do root rodando a cada evento de rede é mais superfície e mais um arquivo para desfazer; há uma janela entre a conexão subir e o script rodar. Não medido.

### C. Só avisar
Manter o comportamento e avisar quando o estado sair de «Protegido».
- Contra: o programa não tem processo residente; avisar exige criar um. E a brecha continua aberta até alguém agir.

## Recomendação (revista depois das medições)
A opção A, que era a recomendada, caiu na medição. Recomendo a **A2** para sistemas com systemd-resolved, mantendo o backend por perfil só onde não há resolved, com a brecha documentada nesse caso até haver medição.

## Decisão (2026-11-15)
Opção A2, com estas escolhas:
1. **Quando vale.** Se o systemd-resolved é quem resolve (está rodando e o `resolv.conf` é o stub `127.0.0.53`), o backend é o dele, haja ou não NetworkManager. Só «estar rodando» não basta. Sem resolved como resolvedor, nada muda: NetworkManager por perfil, com a brecha documentada.
2. **O que é «Protegido».** Com o domínio `~.` no escopo global, o DNS em uso é o global; o DNS do link (o da rede) não entra na conta, porque não responde a nenhuma consulta. Um link que também declare `~.` (uma VPN) entra: se o DNS dele não for o do filtro, o estado cai para «Parcial».
3. **Quem já estava protegido por perfil** (0.3.x, 0.4.0) continua como está até desfazer; o desfazer usa o backend gravado no registro. Ao proteger de novo, passa para o filtro global. O desfazer sem registro tira o drop-in e, se ainda houver DNS do filtro nos perfis, limpa os perfis também.
4. **VPN de trabalho.** Aceito: com o filtro ligado, nomes internos de uma VPN que não declare `~.` deixam de resolver. Sem exceção configurável por enquanto.

## Prova (VM Fedora 43, binário desta mudança)

| Momento | DNS do link | DNS global | Sonda | `guialar status` |
|---|---|---|---|---|
| Antes | `10.0.2.3` | vazio | abre | Não protegido |
| Depois de proteger | `10.0.2.3` | `1.1.1.3 1.0.0.3` + IPv6 | `0.0.0.0` | Protegido |
| Com um perfil novo ativo | `10.0.2.3` | o mesmo | `0.0.0.0` | Protegido |
| Trocado para só anúncios | `10.0.2.3` | `94.140.14.14 94.140.15.15` + IPv6 | abre (esperado nesse modo) | Protegido |
| Depois de desfazer | `10.0.2.3` | vazio | abre | Não protegido; drop-in e registro apagados |

O registro gravou `"backend": "SystemdResolved"`. O e2e (`scripts/e2e.sh`) passou 9/9 no mesmo binário.

## Não coberto
- NetworkManager sem systemd-resolved: a brecha continua.
- O caso «conta comum numa sessão gráfica troca de Wi-Fi» não foi exercitado (a troca de perfil nas medições foi com `sudo`).
- O achado S6 (`uuid` nas chamadas do `nmcli`) fica para depois: com o resolved como resolvedor, o backend por perfil deixa de ser usado nessas máquinas.

# ADR-032 — NetworkManager: uma conexão nova não pode ficar sem filtro

Status: **Proposta** (2026-11-15), aguardando decisão · Origem: revisão de segurança do comportamento (achado S2)

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

### A. DNS global do NetworkManager (recomendada)
Gravar `/etc/NetworkManager/conf.d/90-guialar.conf` com uma seção `[global-dns-domain-*]` e os servidores do modo, e recarregar o NetworkManager. O DNS global vale para todas as conexões, as de agora e as futuras, e se sobrepõe ao DNS de cada perfil.
- A favor: fecha a brecha na origem; o desfazer é apagar um arquivo e recarregar; o registro deixa de precisar do DNS original de cada perfil; cobre também gsm, bluetooth e bridge, que hoje ficam de fora.
- Contra: muda o desenho do backend (e do registro) do NetworkManager; o DNS entregue por uma VPN também é sobreposto (para um filtro familiar isso é desejável, mas quebra nomes internos de VPN de trabalho); precisa ser medido em cada combinação (NetworkManager com e sem systemd-resolved).

### B. Script no dispatcher do NetworkManager
Instalar um script em `/etc/NetworkManager/dispatcher.d/` que reaplica o DNS do filtro a cada conexão que sobe.
- A favor: mantém o desenho atual por perfil.
- Contra: um script do root rodando a cada evento de rede é mais superfície e mais um arquivo para desfazer; há uma janela entre a conexão subir e o script rodar.

### C. Só avisar
Manter o comportamento e avisar quando o estado sair de «Protegido».
- A favor: nenhuma mudança no que roda como root.
- Contra: o programa não tem processo residente; avisar exige criar um (serviço de usuário ou do sistema). E a brecha continua aberta até alguém agir.

## Recomendação
Opção A, com a B descartada e a C como complemento futuro. Antes de implementar: medir na VM Fedora (NetworkManager com systemd-resolved) e numa distribuição sem resolved que `global-dns-domain-*` de fato se sobrepõe ao DNS do perfil e do DHCP, em IPv4 e IPv6.

## Junto com esta correção
- Chamar `nmcli connection modify|show` sempre com a palavra `uuid` antes do identificador (achado S6): sem ela, um perfil cujo **nome** é igual ao UUID de outro poderia ser o escolhido.

## Verificação (quando implementada)
- VM Fedora: proteger, ativar um perfil novo, sonda continua `0.0.0.0` e o estado continua «Protegido»; desfazer devolve o DNS e apaga o arquivo de configuração.
- Testes com o `FakeRunner` para o arquivo gravado, o recarregar e o desfazer.

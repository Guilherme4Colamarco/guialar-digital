# ADR-027 — Texto do Parcial, DNS ilegível, filtro sem manifesto

Status: Aceita (2026-11-11) · Autor: gl-arquiteto · Depois do QA do pacote `87c9c0e`

O qa mediu na void, com `dig`: `1.1.1.2` devolve IP público para `nudity.testcategory.com` e bloqueia `malware.testcategory.com`; `1.1.1.3` bloqueia o nome de nudez. A troca da sonda fica. Isso é medição do qa, não página da Cloudflare.

## Decisão
### 1. Parcial por DNS de fora
A frase não cita "Proteger". Nesse estado os botões são Verificar e Desativar. Texto: desative e proteja de novo.

`guialar apply` com manifesto e DNS fora do modo não responde "já está ativa". Reaplica o DNS daquele modo e mantém o DNS original gravado no manifesto.

### 2. DNS ilegível não é Protegido
Com manifesto, se não há DNS legível fora de stub local (`127.x`, `::1`, `fec0::/10`), o estado é Parcial: "Não consegui ler o DNS que este computador usa." A sonda sozinha não basta. O comentário que diz "vale só a sonda" sai.

### 3. Filtro ligado sem manifesto
Continua Desprotegido no sentido de "não há registro nosso". A frase diz que o filtro ainda está na rede. A ação primária passa a ser Desativar (`revert`), não Proteger, para não gravar o IP do filtro como DNS de antes.

## Fora
Push, tag, release, detector de DoH. P2 e P3 já estão em `5cf1e2d` e não se refazem aqui.

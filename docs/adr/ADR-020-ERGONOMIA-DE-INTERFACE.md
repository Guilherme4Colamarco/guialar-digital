# ADR-020 — Ergonomia da interface gráfica (GTK4/Libadwaita e Win32/Fluent)

Status: Proposto (2026-11-08) · Autor: gl-arquiteto · Complementa ADR-018 e ADR-019
Público-alvo da UI: pais/responsáveis sem perfil técnico, instalando proteção para a família.

## 1. Método

1. **Avaliação heurística** das telas v0.3.0 (print `win11-gui.png`, código `src/ui/*.rs`) contra:
   - 10 heurísticas de Nielsen (1994);
   - Lei de Fitts (tamanho/distância de alvo) e Lei de Hick (nº de escolhas);
   - WCAG 2.2 (contraste 1.4.3, alvo mínimo 2.5.8, foco visível 2.4.7, não depender só de cor 1.4.1);
   - Guias de plataforma: Microsoft Fluent/Windows 11 e GNOME HIG (Libadwaita).
2. **Walkthrough cognitivo** da tarefa nuclear: "quero proteger este computador" em 3 perguntas por passo (sei o que fazer? vejo como? entendo o retorno?).
3. **Limite honesto:** não houve teste com usuários reais. A Seção 5 define como validar com pessoas.
4. **Verificação das fontes (2026-11-08):** as referências foram conferidas ao vivo nas páginas oficiais; veja `ADR-020-REFERENCIAS-VERIFICADAS.md`. Exceções: Hick 1952 e Bangor 2008 (bloqueio anti-robô; só metadados), Sauro 2011 e Tullis & Stetson 2004 (citados dentro de Brooke 2013; não lidos), capítulo original de Brooke 1996 (não lido), WCAG 4.1.2 (não consultado).
5. Critérios WCAG 2.2 usados: 1.4.1 (A), 1.4.3 (AA, 4,5:1), 1.4.11 contraste não textual (AA, 3:1), 2.4.7 (AA), 2.4.11 foco não obscurecido (AA) e 2.5.8 (AA, 24x24 CSS px; em app nativo é equivalência aproximada).

## 2. Achados (ordenados por impacto)

| # | Achado | Heurística | Gravidade |
|---|--------|-----------|-----------|
| E1 | Todas as 4 ações aparecem sempre, inclusive "Restaurar" sem proteção ativa (pedido do usuário, já em andamento) | N5 prevenção de erro, N8 minimalismo, Hick | Alta |
| E2 | Nenhuma ação é primária: "Ativar Proteção" tem o mesmo peso visual (contorno) que "Verificar" | Hierarquia visual, Fitts | Alta |
| E3 | Título do card repete o rótulo do botão ("Verificar Agora" / "Verificar Agora") | N8 | Média |
| E4 | Modelo mental confuso: "Ativar Proteção Completa" já inclui o bloqueador, mas "Proteger Navegadores" é outro card | N2 correspondência com o mundo real, Hick | Alta |
| E5 | "Alternar tema" no topo, com peso igual às ações, e o app já segue o tema do sistema (ADR-019) | N8, N4 consistência | Média |
| E6 | Jargão (DNS, uBlock, Cloudflare, manifesto) no nível principal; no Linux há expander, no Windows não | N2 | Média |
| E7 | Feedback fraco: linha "Pronto." em cinza; sem indicador de progresso nem botões desabilitados durante a ação | N1 visibilidade do estado | Alta |
| E8 | Erro mostra a cadeia crua do `anyhow` (`Não foi possível concluir: {e:#}`) | N9 mensagens de erro | Média |
| E9 | Estado vermelho "Computador Desprotegido" sozinho, sem dizer o próximo passo | N1/N9 | Média |
| E10 | Alvos de clique ~30 px de altura no Windows | Fitts, WCAG 2.5.8 (mín. 24, ok) · Microsoft: alvo mínimo 40x40 epx, ou 32 epx de altura quando a largura é ≥ 120 epx (44x44 para toque) | Baixa |
| E11 | Não verificado: ordem de Tab, botão padrão (Enter), nomes acessíveis p/ leitor de tela, atalhos, foco visível, zoom/DPI 150–200 % | WCAG 2.4.7, WCAG 4.1.2 (não consultado), HIG: Accessible Names; N7 só para atalhos | Desconhecida: exige teste |

Pontos que já estão certos: escudo + texto + cor juntos (não depende só de cor), escudo UAC nos botões elevados, Mica/tema do sistema, fallback para CLI.

## 3. Decisão — modelo de tela orientado por estado

Uma tela, 4 estados, **no máximo 1 ação primária e 1 secundária visíveis**.

| Estado | Cabeçalho | Primária (destaque/accent) | Secundária | Oculto |
|--------|-----------|---------------------------|------------|--------|
| Desprotegido | "Este computador ainda não está protegido" + 1 frase | **Proteger este computador** (escudo UAC) | Verificar | Desativar |
| Protegido | "Computador protegido" (verde, ícone ✓) | Verificar novamente | Desativar proteção | Proteger |
| Parcial (DNS ok, navegador sem bloqueador) | "Proteção incompleta" + o que falta | **Proteger navegadores** | Desativar proteção | — |
| Trabalhando | "Aplicando…" + barra indeterminada | todos os botões desabilitados | — | — |

Regras:
1. **Um conceito, um nome.** "Proteger este computador" faz DNS + navegadores numa só ação. O card "Proteger Navegadores" separado vira linha de **status por navegador** (ex.: "Edge: protegido", "Chrome: não instalado") dentro de "Detalhes", e só vira botão no estado Parcial.
2. **Rótulo do botão = verbo + objeto; título do card = estado/descrição**, nunca iguais.
3. **Remover "Alternar tema"**; o tema segue o sistema (ADR-019).
4. **Linguagem simples no nível 1**; DNS/Cloudflare/uBlock/manifesto só em "Detalhes técnicos" (expander no Linux; seção recolhível no Windows).
5. **Feedback:** durante a ação, botões desabilitados + indicador de ocupado + texto do que está acontecendo. Linux: **spinner** (HIG: tarefas até ~30 s; progresso só se passar de ~3 s); barra de progresso só para tarefa longa. Windows: indicação de ocupado (busy), pois a ação não é instantânea. Resultado em frase curta: Toast no Linux (com "Desfazer" quando couber), linha de status com ícone no Windows. "Proteção incompleta" no Linux: considerar **banner** (estado persistente), não só texto.
6. **Erros:** frase humana + próximo passo; a cadeia técnica vai em "Detalhes" com botão "Copiar detalhes".
7. **Desativar proteção** pede confirmação curta só se houver mais de um usuário/estado a perder; caso contrário, executa e mostra "Desfazer = Proteger de novo" (ação reversível, sem diálogo).
8. **Alvos:** botões com altura ≥ 36 px no Windows (a 100 % DPI). **Isto é decisão de projeto**, entre os 32 epx e os 40 epx da Microsoft, e não um número da Microsoft. Espaçamento ≥ 8 px entre botões (Fluent: 8 epx).
9. **Teclado e acessibilidade (obrigatório):**
   - Tab percorre na ordem visual.
   - Enter ativa a primária. O botão padrão é a ação segura e reversível; nunca "Desativar proteção" sem desfazer fácil.
   - Esc fecha/cancela diálogos, popovers e menus (UI transitória); não fecha a janela principal.
   - Atalho `Ctrl+R` para verificar. O HIG define Ctrl+R como "Recarregar" e não há atalho padrão para "verificar"; manter, mas documentar.
   - Foco visível.
   - Nome acessível em todo botão e ícone de estado. Linux: `gtk_accessible_update_property(..., GTK_ACCESSIBLE_PROPERTY_LABEL, ...)` (interface `GtkAccessible`). Windows: nome UIA = texto visível do botão; ícone/escudo/estado sem texto precisa de nome alternativo curto; o nome não inclui o papel ("botão"). **Não verificado** que `SetWindowText` baste para o nome UIA; o ícone de estado em GDI não expõe nome.
   - Contraste de texto ≥ 4,5:1 e de ícones/componentes ≥ 3:1 (1.4.11), em claro e escuro.
   - Funcional a 150 % e 200 % de escala.
10. **Estado sempre atualizado:** reverificar ao abrir e depois de cada ação; nunca mostrar botão de um estado que já não vale (liga com o achado de revisão sobre sucesso falso em `action_activate`).

## 4. Consequências

- O refatorador altera só `src/ui/*` (mais textos em `ui/mod.rs`); o contrato de CLI não muda.
- A lógica de estado deve ficar em `ui/mod.rs` (um `enum UiState` + função `actions_for(state)`), testável sem GUI; os dois backends só desenham. Isso evita divergência Linux x Windows.
- Custo: ~1 sprint curta. Risco: o estado Parcial exige o detector de navegadores devolver "protegido/não" por navegador, não só "detectado".

## 5. Validação (critérios de aceite)

Automatizável (gl-refatorador, testes unitários): `actions_for(estado)` devolve exatamente o conjunto da tabela da Seção 3.

Com as VMs (gl-qa, Windows 11 + Fedora, claro e escuro, 100 % e 150 % DPI): checklist E1–E11, com print por estado, e teste só de teclado.

Com pessoas (fora do alcance dos agentes; depende do Guilherme): 3–5 pessoas não técnicas, tarefa "proteja este computador e confirme que funcionou". Medir:
- sucesso sem ajuda (meta ≥ 80 %);
- cliques até concluir (meta ≤ 2);
- tempo até a primeira ativação;
- questionário SUS. Referência de média ~68–70 (Brooke 2013, citando Sauro 2011; Bangor 2009: 69,5). Meta ≥ 80 corresponde a "good"/nota B (Bangor 2009) e fica acima do terceiro quartil de 77,8. Com 3–5 pessoas o SUS é só indicativo, não prova: documentar o N.
Esses números não existem ainda. Não serão reportados sem a rodada real.

## 6. Referências (verificadas em 2026-11-08; URLs específicos e exceções em `ADR-020-REFERENCIAS-VERIFICADAS.md`)

- NIELSEN, J. *10 Usability Heuristics for User Interface Design*. Nielsen Norman Group, 24/04/1994, última revisão 30/01/2024. https://www.nngroup.com/articles/ten-usability-heuristics/
- FITTS, P. M. The information capacity of the human motor system in controlling the amplitude of movement. *J. Exp. Psychol.*, 47(6), 381–391, 1954. doi:10.1037/h0055392
- HICK, W. E. On the rate of gain of information. *Q. J. Exp. Psychol.*, 4(1), 11–26, 1952. doi:10.1080/17470215208416600 (texto não lido; só metadados)
- W3C. *Web Content Accessibility Guidelines (WCAG) 2.2*. Recomendação de 05/10/2023, republicada em 12/12/2024. https://www.w3.org/TR/WCAG22/
- Microsoft Learn (Windows / Fluent) e GNOME HIG: páginas específicas listadas no relatório de verificação. Parte do guia Win32 citado é do Windows 7 e declara-se desatualizada: serve de apoio, não de regra do Windows 11.
- BROOKE, J. SUS: a "quick and dirty" usability scale. In JORDAN, P. W. et al. (Eds.), *Usability Evaluation in Industry*. Taylor & Francis, 1996, p. 189–194.
- BANGOR, A.; KORTUM, P.; MILLER, J. Determining what individual SUS scores mean. *J. Usability Studies*, 4(3), 114–123, 2009.
- BROOKE, J. SUS: a retrospective. *J. Usability Studies*, 8(2), 29–40, 2013.

# Entrega GuiaLar Digital - Base MVP

**Data:** 22 de setembro de 2026  
**Projeto:** Projetos Integrados I - Uniube  
**Aluno:** Guilherme Amaral Colamarco Resende de Melo

---

## 📦 O Que Foi Entregue

### 1. Base do Projeto Java Funcional

✅ **Build System:** Apache Ant (build.xml)
- Compilação: `ant jar`
- Execução: `sudo java -jar build/jar/guialar-digital.jar`
- Clean/Rebuild: `ant clean`, `ant rebuild`

✅ **Arquitetura Modular:**
```
domínio/
  ├── sistema/      → Tipos de SO (Linux/Windows)
  ├── distro/       → Distribuições (Debian/Fedora/Arch)
  ├── dns/          → DNS Cloudflare Families
  ├── navegador/    → Navegadores e extensões
  └── autorizacao/  → Plano de ações

aplicacao/
  ├── deteccao/     → Detecção SO/distro/navegadores
  ├── dns/          → Configuradores (Linux + Windows esboço)
  ├── extensao/     → Instalador uBlock Origin/Lite
  └── autorizacao/  → Sistema de autorização

cli/
  └── GuiaLarCli    → Interface linha de comando
```

---

## 🎯 Funcionalidades MVP Linux (Completo)

### 1. Detecção de Distribuição
- ✅ Debian 11+ / Ubuntu 20.04+
- ✅ Fedora 38+
- ✅ Arch Linux
- ✅ Identifica gerenciador de rede (NetworkManager, systemd-resolved, Netplan)

### 2. Configuração DNS Cloudflare Families
- ✅ **IPv4:** 1.1.1.3 (primário) / 1.0.0.3 (secundário)
- ✅ **IPv6:** 2606:4700:4700::1113 / 2606:4700:4700::1003
- ✅ Proteção: Bloqueia malware + conteúdo adulto (18+)
- ✅ Backup automático antes de modificar
- ✅ Instruções de reversão

### 3. Detecção de Navegadores
- ✅ **Firefox family:** Firefox, Firefox ESR, LibreWolf, Waterfox
- ✅ **Chromium family:** Chromium, Chrome, Brave, Edge, Vivaldi, Opera
- ✅ Identifica perfis e caminhos

### 4. Instalação de Adblockers
- ✅ **Firefox:** uBlock Origin (via policies.json)
- ✅ **Chromium:** uBlock Origin Lite (instruções + arquivo)

### 5. Sistema de Autorização (UX/Segurança)
- ✅ **Plano de ações transparente** listado ANTES de executar
- ✅ Solicita confirmação explícita do usuário (s/N)
- ✅ Verifica privilégios administrativos (sudo/polkit)
- ✅ Nunca executa mudanças silenciosamente

---

## 🔧 Windows (Esboço Inicial)

✅ Detecção básica de Windows  
✅ Instruções manuais PowerShell (Set-DnsClientServerAddress)  
✅ Notas sobre DoH no Edge/Chrome  
⚠️ Implementação automática não completa (escopo fechado pelo usuário)

---

## 📚 Documentação Entregue

### README.md (Português)
- ✅ Sobre o projeto e escopo
- ✅ Como compilar com Ant
- ✅ Como executar (Linux)
- ✅ Testes e verificação
- ✅ Smoke tests (dig / testcategory.com)
- ✅ Compatibilidade (tabela de suporte)
- ✅ Limitações conhecidas (VPN, Docker, DoH)
- ✅ Instruções de reversão

### TESTING.md
- ✅ Guia completo de testes
- ✅ Pré-requisitos (dnsutils, bind-utils)
- ✅ Fluxo de teste esperado
- ✅ Verificação pós-instalação
- ✅ Smoke tests com dig
- ✅ Diagnóstico de problemas
- ✅ Checklist de testes

### build.xml
- ✅ Targets: clean, compile, jar, dist, run, rebuild, help
- ✅ Java 17
- ✅ Manifesto com Main-Class
- ✅ Comentários e documentação

### .gitignore
- ✅ Ignora build/, dist/, lib/
- ✅ Ignora IDEs e arquivos temporários

---

## 🔒 Requisitos de Segurança/Privacidade Atendidos

### Fluxo Obrigatório Implementado:
1. ✅ Detecta sistema e navegadores
2. ✅ Monta plano de ações
3. ✅ **Exibe plano completo** (listagem clara)
4. ✅ Solicita autorização explícita
5. ✅ Eleva privilégios (sudo/polkit)
6. ✅ Faz backup
7. ✅ Executa ações
8. ✅ Mostra como reverter

### Princípios:
- ✅ Sem coleta de histórico de navegação
- ✅ Código aberto e auditável
- ✅ Regras transparentes (sem ML caixa-preta)
- ✅ Proteção sem vigilância excessiva
- ✅ Reversibilidade total

---

## ✅ Critérios de Sucesso Validados

- [x] Compila sem erros: `ant jar` ✓
- [x] Estrutura modular clara (domínio/aplicação/CLI) ✓
- [x] Classes para diagnóstico (dispositivo/SO/navegador) ✓
- [x] Motor de recomendação (regras transparentes - DNS Families) ✓
- [x] Tutoriais/configurações guiados (DNS + extensões) ✓
- [x] Verificação (instruções dig/testcategory) ✓
- [x] Feedback/resultado (plano + instruções reversão) ✓
- [x] Instruções claras de execução (README em português) ✓
- [x] Base real utilizável (não apenas stubs vazios) ✓
- [x] Nada fora do MVP (sem Python, sem coleta de histórico) ✓

---

## 🧪 Smoke Tests Prontos

```bash
# 1. Compilar
ant jar

# 2. Executar (requer sudo)
sudo java -jar build/jar/guialar-digital.jar

# 3. Verificar DNS
resolvectl status
# ou
nmcli device show | grep DNS

# 4. Testar bloqueio
dig @1.1.1.3 malware.testcategory.com  # deve retornar 0.0.0.0
dig @1.1.1.3 adult.testcategory.com    # deve retornar 0.0.0.0
dig @1.1.1.3 example.com               # deve funcionar

# 5. Verificar navegadores
ls ~/.mozilla/firefox/
ls ~/.config/chromium/
ls ~/.config/google-chrome/
```

---

## 📋 Escopo Respeitado

### Seguido Estritamente:
✅ Estrutura oficial do aluno (A1/A2)  
✅ Java (não Python)  
✅ Apache Ant (não Maven/Gradle)  
✅ Linux: Debian, Fedora, Arch ("trindade sagrada")  
✅ DNS Cloudflare Families (1.1.1.3 / 1.0.0.3 + IPv6)  
✅ Navegadores Firefox/Chromium  
✅ uBlock Origin / uBlock Origin Lite  
✅ Plano transparente + autorização polkit  
✅ Windows esboço (não bloqueia Linux)  

### Fora do Escopo (Conforme Decisão):
❌ macOS  
❌ Android/iOS  
❌ Python  
❌ Maven/Gradle  
❌ Controle parental invasivo  
❌ Coleta de histórico de navegação  

---

## 🔗 Links

- **Repositório:** https://github.com/Guilherme4Colamarco/guialar-digital
- **Pull Request:** https://github.com/Guilherme4Colamarco/guialar-digital/pull/1
- **Branch:** `cursor/projeto-base-guialar-97b5`

---

## 📊 Estatísticas

- **Arquivos Java:** 18 classes
- **Linhas de código:** ~2.260 linhas
- **Módulos principais:** 5 (sistema, distro, DNS, navegador, autorização)
- **Documentação:** 3 arquivos (README, TESTING, ENTREGA)
- **Tempo de compilação:** < 1 segundo
- **Tamanho JAR:** ~15 KB (sem dependências externas)

---

## ✨ Próximos Passos Sugeridos (Fora desta Entrega)

1. Testar em Fedora e Arch (atualmente testado em Ubuntu/Debian)
2. Implementar detecção de navegadores via .desktop files
3. Adicionar suporte a Flatpak/Snap
4. Expandir esboço Windows (automação PowerShell)
5. Adicionar testes unitários
6. Melhorar detecção de DoH habilitado nos navegadores
7. Criar script de desinstalação/limpeza

---

**Entregue em:** 22/09/2026  
**Status:** ✅ MVP Base Completo  
**PR:** #1 (draft aberto)

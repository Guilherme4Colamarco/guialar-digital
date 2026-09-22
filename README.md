# GuiaLar Digital

[![Build and Test](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/build-and-test.yml/badge.svg)](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/build-and-test.yml)
[![Quick Check](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/quick-check.yml/badge.svg)](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/quick-check.yml)

**Assistente para configuração de DNS seguro e adblockers (Linux + esboço Windows)**

Projeto desenvolvido como parte da disciplina Projetos Integrados I  
**Curso:** Inteligência Artificial e Ciência de Dados — Uniube  
**Aluno:** Guilherme Amaral Colamarco Resende de Melo

---

## 📋 Sobre o Projeto

O GuiaLar Digital é uma aplicação Java que automatiza a configuração de proteções básicas de privacidade e segurança, com foco em **Linux** (MVP completo) e esboço para **Windows**.

### Escopo do Projeto:

**MVP COMPLETO (Linux):**
- ✅ Debian, Fedora e Arch Linux (a "trindade sagrada")
- ✅ Configuração completa de DNS + navegadores + extensões
- ✅ Testes e verificação

**ESBOÇO INICIAL (Windows):**
- 🔧 Interface básica para Windows (PowerShell/Set-DnsClientServerAddress)
- 🔧 Estrutura preparada, implementação mínima

**FORA DO ESCOPO (por enquanto):**
- ❌ macOS
- ❌ Android/iOS

### Funcionalidades do MVP Linux:

1. **Mudar DNS do sistema** para Cloudflare 1.1.1.1 for Families
   - **IPv4:** 1.1.1.3 (primário) / 1.0.0.3 (secundário)
   - **IPv6:** 2606:4700:4700::1113 / 2606:4700:4700::1003
   - Bloqueia malware e conteúdo adulto (18+)
   
2. **Detectar navegadores instalados** e identificar base Firefox ou Chromium

3. **Instalar adblocker** automaticamente:
   - uBlock Origin para navegadores base Firefox
   - uBlock Origin Lite para navegadores base Chromium

### Princípios de Segurança e Privacidade:

- ✅ **Plano de ações transparente:** sempre lista o que será feito ANTES de executar
- ✅ **Autorização explícita:** solicita confirmação via polkit/pkexec
- ✅ **Backup automático:** faz backup antes de modificar configurações
- ✅ **Sem coleta de dados:** nenhum histórico de navegação é coletado
- ✅ **Reversível:** instruções claras de como reverter mudanças

---

## 🚀 Como Executar

### Pré-requisitos

- **Java 17** ou superior
- **Apache Ant 1.10+** (sistema de build)
- **Sistema operacional:** Debian, Fedora ou Arch Linux (MVP completo)
- **Privilégios:** sudo/root para alterar DNS do sistema

### Instalação e Compilação

1. **Clone o repositório:**
   ```bash
   git clone <url-do-repositorio>
   cd workspace
   ```

2. **Compile o projeto com Ant:**
   ```bash
   ant jar
   ```
   
   Ou para limpar e recompilar tudo:
   ```bash
   ant rebuild
   ```

3. **Execute a aplicação:**
   ```bash
   sudo java -jar build/jar/guialar-digital.jar
   ```
   
   Ou use o Ant diretamente:
   ```bash
   sudo ant run
   ```

   > **Nota:** É necessário `sudo` para alterar as configurações de DNS do sistema.

### Comandos Ant Disponíveis

```bash
ant help      # Mostra ajuda sobre os comandos
ant clean     # Remove arquivos compilados
ant compile   # Compila o código-fonte
ant jar       # Cria o JAR executável (padrão)
ant dist      # Cria distribuição completa
ant run       # Executa a aplicação em modo CLI (requer sudo)
ant run-gui   # Executa a interface gráfica (Swing)
ant rebuild   # Limpa e reconstrói tudo
```

### Interface Gráfica (GUI)

Além da CLI, o GuiaLar Digital possui uma **interface gráfica** (Java Swing, sem
dependências externas). Ela exibe o sistema detectado, os navegadores, o plano de
ações e um painel de log da execução em tempo real.

```bash
# Compila e abre a interface gráfica
ant run-gui

# Ou diretamente pelo JAR (com privilégios para alterar o DNS):
sudo java -jar build/jar/guialar-digital.jar --gui
```

Seleção de modo ao executar o JAR:

- `--gui` força a interface gráfica.
- `--cli` força o modo linha de comando.
- Sem flags: usa a GUI quando há ambiente gráfico disponível e cai para a CLI em
  ambientes headless (servidores, CI, containers).

---

## 🧪 Como Testar

### Teste Completo no Linux

#### 1. Compilar e Executar

```bash
# Compilar
ant jar

# Executar (requer sudo)
sudo java -jar build/jar/guialar-digital.jar
```

O programa irá:
1. Detectar sua distribuição Linux
2. Detectar navegadores instalados
3. Exibir um **plano de ações** detalhado
4. Solicitar sua autorização
5. Executar as configurações

#### 2. Verificar DNS Configurado

**Debian/Ubuntu:**
```bash
# Verificar DNS
systemd-resolve --status | grep "DNS Servers"
# ou
resolvectl status
# ou
cat /etc/resolv.conf

# Teste com dig
dig @1.1.1.3 example.com
```

**Fedora:**
```bash
# Verificar DNS
nmcli device show | grep DNS

# Teste com dig
dig @1.1.1.3 example.com
```

**Arch Linux:**
```bash
# Verificar DNS
resolvectl status

# Teste com dig
dig @1.1.1.3 example.com
```

#### 3. Verificação Automática (Smoke Test Integrado)

**O GuiaLar Digital executa verificação automática pós-DNS!**

Após configurar o DNS, o programa automaticamente:
- ✅ Testa `malware.testcategory.com` (IPv4 e IPv6)
- ✅ Testa `nudity.testcategory.com` (IPv4 e IPv6)
- ✅ Testa `example.com` (IPv4 e IPv6)
- ✅ Exibe relatório de sucesso/falha

**Resultado esperado no programa:**

```
🧪 ETAPA 4: Verificação DNS (Smoke Test)...
─────────────────────────────────────────────────────────────────

🔍 Verificando DNS Cloudflare Families...
─────────────────────────────────────────────────────────────────

  Testando malware.testcategory.com (IPv4)... ✅ Bloqueado (0.0.0.0)
  Testando malware.testcategory.com (IPv6)... ✅ Bloqueado (::)
  Testando nudity.testcategory.com (IPv4)... ✅ Bloqueado (0.0.0.0)
  Testando nudity.testcategory.com (IPv6)... ✅ Bloqueado (::)
  Testando example.com (IPv4)... ✅ Permitido (93.184.215.14)
  Testando example.com (IPv6)... ✅ Permitido (2606:2800:21f:cb07:6820:80da:af6b:8b2c)

─────────────────────────────────────────────────────────────────
RESUMO DA VERIFICAÇÃO:
─────────────────────────────────────────────────────────────────

Total de testes: 6
Sucesso: 6
Falhas: 0

✅ TODOS OS TESTES PASSARAM!
   O DNS Cloudflare Families está funcionando corretamente.
```

#### Teste Manual (Linha de Comando)

**IPv4:**
```bash
# Teste 1: Bloqueio de malware (deve retornar 0.0.0.0)
dig @1.1.1.3 malware.testcategory.com +short

# Teste 2: Bloqueio de conteúdo adulto/nudez (deve retornar 0.0.0.0)
dig @1.1.1.3 nudity.testcategory.com +short

# Teste 3: Site normal (deve funcionar)
dig @1.1.1.3 example.com +short
```

**IPv6:**
```bash
# Teste 1: Bloqueio de malware (deve retornar ::)
dig @2606:4700:4700::1113 malware.testcategory.com AAAA +short

# Teste 2: Bloqueio de nudez (deve retornar ::)
dig @2606:4700:4700::1113 nudity.testcategory.com AAAA +short

# Teste 3: Site normal (deve funcionar)
dig @2606:4700:4700::1113 example.com AAAA +short
```

**Alternativa com resolvectl (systemd-resolved):**
```bash
resolvectl query malware.testcategory.com
resolvectl query nudity.testcategory.com
resolvectl query example.com
```

**Resultado esperado:**

```bash
# IPv4 - Malware e nudez BLOQUEADOS
$ dig @1.1.1.3 malware.testcategory.com +short
0.0.0.0

$ dig @1.1.1.3 nudity.testcategory.com +short
0.0.0.0

# IPv6 - Malware e nudez BLOQUEADOS
$ dig @2606:4700:4700::1113 malware.testcategory.com AAAA +short
::

# IPv4/IPv6 - Site normal PERMITIDO
$ dig @1.1.1.3 example.com +short
93.184.215.14

$ dig @2606:4700:4700::1113 example.com AAAA +short
2606:2800:21f:cb07:6820:80da:af6b:8b2c
```

**Teste no navegador:**

```bash
# Deve mostrar página de bloqueio ou erro de conexão
curl -I http://malware.testcategory.com
curl -I http://nudity.testcategory.com

# Deve funcionar normalmente
curl -I http://example.com
```

**✅ Checklist de Verificação:**
- ✅ `malware.testcategory.com` → 0.0.0.0 (IPv4) ou :: (IPv6)
- ✅ `nudity.testcategory.com` → 0.0.0.0 (IPv4) ou :: (IPv6)
- ✅ `example.com` → IP válido funcionando
- ✅ Smoke test automático passou no programa

#### 4. Instalar Navegadores para Teste (Opcional)

```bash
# Debian/Ubuntu:
sudo apt install firefox chromium-browser

# Fedora:
sudo dnf install firefox chromium

# Arch:
sudo pacman -S firefox chromium
```

---

## 📁 Estrutura do Projeto

### Arquitetura: Core Comum + Adaptadores por OS

O projeto segue uma **arquitetura limpa** com interfaces comuns e implementações específicas por sistema operacional.

**MVP:** Apenas Linux implementado  
**Futuro:** Interfaces definidas para Mac, Windows e Android (sem código funcional)

```
src/main/java/br/uniube/pi/guialar/
├── GuiaLarApplication.java              # Classe principal
├── dominio/                             # Core comum (interfaces e modelos)
│   ├── sistema/
│   │   ├── TipoSistema.java            # Enum: LINUX, WINDOWS, MAC...
│   │   └── SistemaOperacional.java     # Interface base OS
│   ├── adaptadores/                     # ⭐ Interfaces por OS
│   │   ├── DnsChanger.java             # Interface troca DNS
│   │   ├── BrowserDetector.java        # Interface detecção browsers
│   │   └── ExtensionInstaller.java     # Interface instalação extensões
│   ├── distro/
│   │   ├── TipoDistro.java             # Enum: DEBIAN, FEDORA, ARCH
│   │   └── InfoDistro.java             # Informações da distribuição
│   ├── dns/
│   │   ├── ConfiguracaoDns.java        # Modelo de configuração DNS
│   │   └── ServidorDns.java            # Servidores DNS (Cloudflare Families)
│   ├── navegador/
│   │   ├── TipoNavegador.java          # Enum: FIREFOX, CHROMIUM
│   │   ├── Navegador.java              # Modelo de navegador detectado
│   │   └── ExtensaoAdblocker.java      # uBlock Origin / uBlock Origin Lite
│   ├── verificacao/
│   │   └── ResultadoVerificacao.java   # Resultado smoke tests
│   └── autorizacao/
│       └── PlanoAcao.java              # Plano de ações transparente
├── aplicacao/                           # Implementações
│   ├── adaptadores/                     # ⭐ Implementações por OS
│   │   └── linux/                      # ✅ MVP: ÚNICO OS IMPLEMENTADO
│   │       ├── LinuxDnsChanger.java    # NetworkManager/systemd-resolved
│   │       ├── LinuxBrowserDetector.java # PATH/.desktop/perfis
│   │       └── LinuxExtensionInstaller.java # policies.json
│   │   ├── macos/                       # 🔧 Futuro: apenas design
│   │   ├── windows/                     # 🔧 Futuro: apenas design
│   │   └── android/                     # 🔧 Futuro: apenas design
│   ├── deteccao/
│   │   ├── DetectorSistemaService.java # Detecta Linux/Windows/Mac
│   │   ├── DetectorDistroService.java  # Detecta Debian/Fedora/Arch
│   │   └── DetectorNavegadorService.java # Detecta Firefox/Chromium
│   ├── dns/
│   │   ├── ConfiguradorDnsService.java # Troca DNS Linux
│   │   └── ConfiguradorDnsWindowsService.java # Esboço Windows
│   ├── extensao/
│   │   └── InstaladorExtensaoService.java # Instala uBlock Origin/Lite
│   ├── verificacao/
│   │   └── VerificacaoDnsService.java  # Smoke tests pós-DNS
│   └── autorizacao/
│       └── AutorizadorService.java     # Sistema autorização
└── cli/
    └── GuiaLarCli.java                  # Interface linha de comando
```

---

## 🏗️ Arquitetura Multi-OS (Design Futuro)

### Interfaces Definidas (Core Comum)

| Interface | Responsabilidade | Implementações |
|-----------|-----------------|----------------|
| `SistemaOperacional` | Detectar OS | Linux ✅, Mac 🔧, Windows 🔧, Android 🔧 |
| `DnsChanger` | Trocar DNS | LinuxDnsChanger ✅ |
| `BrowserDetector` | Detectar browsers | LinuxBrowserDetector ✅ |
| `ExtensionInstaller` | Instalar extensões | LinuxExtensionInstaller ✅ |

### Diferenças Técnicas por OS (Documentadas)

#### ✅ Linux (MVP - Implementado)
- **DNS:** NetworkManager (nmcli), systemd-resolved (resolvectl), Netplan
- **Browsers:** PATH `/usr/bin/`, `.desktop` files, perfis `~/.mozilla/`, `~/.config/`
- **Extensões:** `policies.json` (Firefox), managed policies (Chromium)
- **Distros:** Debian, Ubuntu, Fedora, Arch

#### 🔧 macOS (Futuro - Interface apenas)
- **DNS:** `networksetup`, `scutil` (**≠** NetworkManager/systemd-resolved!)
- **Browsers:** `/Applications/*.app/`, Homebrew `/opt/homebrew/Caskroom/`
- **Perfis:** `~/Library/Application Support/Firefox/`, `~/Library/Application Support/Google/Chrome/`
- **Extensões:** Similar a Linux mas caminhos diferentes

#### 🔧 Windows (Futuro - Interface apenas)
- **DNS:** `netsh`, PowerShell `Set-DnsClientServerAddress` (**≠** nmcli!)
- **Browsers:** `C:\Program Files\`, `C:\Program Files (x86)\`, Registry
- **AppData:** `%LOCALAPPDATA%\`, `%APPDATA%\`
- **Extensões:** Registry `HKLM\SOFTWARE\Policies\`

#### 🔧 Android (Futuro - Interface apenas)
- **DNS:** Private DNS over TLS (sem root, **≠** desktop)
  - Não muda DNS do sistema como desktop
  - Usuário configura manualmente em Settings
- **Browsers:** `pm list packages` (sem acesso a perfis sem root)
- **Extensões:** Instalação manual via store (Play Store, F-Droid)
- **Limitações severas:** Sem root = sem controle total

---

## 🎯 Arquitetura - Módulos Linux (MVP)

### 1. Detecção de Distribuição
Identifica se o sistema é Debian, Fedora ou Arch Linux através de:
- `/etc/os-release`
- `/etc/debian_version`, `/etc/fedora-release`, `/etc/arch-release`

### 2. Troca de DNS do Sistema
Configura DNS seguro (Cloudflare 1.1.1.1 for Families) de acordo com a distro:
- **Debian/Ubuntu:** NetworkManager ou `/etc/resolv.conf` + resolvconf
- **Fedora:** NetworkManager (nmcli)
- **Arch:** systemd-resolved ou NetworkManager

### 3. Detecção de Navegador
Identifica navegadores instalados e sua base:
- **Base Firefox:** Firefox, Firefox ESR, Librewolf, Waterfox
- **Base Chromium:** Chromium, Google Chrome, Brave, Edge, Vivaldi, Opera

### 4. Instalação de Extensões
Instala adblocker apropriado:
- **Firefox:** uBlock Origin (addon oficial)
- **Chromium:** uBlock Origin Lite (Manifest V3)

---

## 🛠️ Tecnologias Utilizadas

- **Java 17** - Linguagem de programação
- **Apache Ant** - Sistema de build (build.xml)
- **Arquitetura modular** - Separação clara entre domínio, aplicação e interface

---

## 📋 Compatibilidade e Matriz de Testes

### Distribuições Linux Suportadas

| Distribuição | Família | Status | Detecção | DNS | Navegadores | Extensões | Testado |
|--------------|---------|--------|----------|-----|-------------|-----------|---------|
| **Debian 11+** | Debian | ✅ Completo | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Ubuntu 20.04+** | Debian | ✅ Completo | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Big Linux** | Debian (baseado Ubuntu) | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |
| **Kali Linux** | Debian | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |
| **Fedora 38+** | Fedora | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |
| **Arch Linux** | Arch | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |
| **EndeavourOS** | Arch | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |
| **CachyOS** | Arch | ✅ Completo | ✅ | ✅ | ✅ | ✅ | 🔧 A testar |

**Nota sobre distribuições:**
- **Big Linux:** Baseado em Ubuntu, distribuição nacional brasileira - alvo prioritário público geral
- **Kali Linux:** Baseado em Debian, caso extremo de segurança - não é proxy do Arch
- **EndeavourOS/CachyOS:** Família Arch, alternativas populares ao Arch vanilla

### Outras Plataformas

| Sistema Operacional | Status | Notas |
|---------------------|--------|-------|
| **Windows 10/11**   | 🔧 Esboço | Instruções PowerShell manuais |
| **macOS**           | ❌ Fora do escopo | Não planejado |
| **Android/iOS**     | ❌ Fora do escopo | Não planejado |

### Gerenciadores de Rede Suportados (Linux)

- ✅ **NetworkManager** (nmcli) - Padrão Fedora, Ubuntu Desktop, Arch (instalável)
- ✅ **systemd-resolved** (resolvectl) - Padrão Ubuntu moderno, Arch
- ✅ **Netplan** - Ubuntu Server/Cloud moderno
- ✅ **/etc/resolv.conf** (fallback manual) - Qualquer distro

---

## 🧪 Matriz de Testes

### Ambientes de Teste Recomendados

O GuiaLar Digital foi projetado para ser testado em diferentes ambientes, cada um com suas vantagens:

#### 1. Docker (Smoke Tests Rápidos)

**Bom para:**
- ✅ Smoke test do JAR Java
- ✅ Detecção de pacotes instalados
- ✅ Detecção de navegadores
- ✅ Verificação de compilação

**Limitações:**
- ⚠️ Sem systemd completo por padrão
- ⚠️ NetworkManager/systemd-resolved não funcionam completamente
- ⚠️ Configuração DNS limitada

**Exemplo Dockerfile:**
```dockerfile
FROM debian:12
RUN apt-get update && apt-get install -y \
    openjdk-17-jdk \
    ant \
    dnsutils \
    firefox-esr \
    chromium
COPY . /app
WORKDIR /app
RUN ant jar
CMD ["java", "-jar", "build/jar/guialar-digital.jar"]
```

#### 2. VM com systemd (QEMU/libvirt/VirtualBox)

**Bom para:**
- ✅ Testes completos de troca de DNS
- ✅ NetworkManager funcional
- ✅ systemd-resolved funcional
- ✅ Ambiente mais fiel ao uso real
- ✅ Testes de navegadores com interface gráfica

**Recomendado para:**
- Testes finais antes de release
- Validação em distribuições específicas (Big Linux, EndeavourOS, etc.)
- Testes de integração completa

**Distribuições para teste prioritário:**
1. **Ubuntu 24.04 LTS** (base comum)
2. **Big Linux** (público brasileiro, baseado Ubuntu)
3. **Fedora 40** (família RHEL)
4. **EndeavourOS** ou **CachyOS** (família Arch user-friendly)
5. **Kali Linux** (caso extremo de segurança)

#### 3. Container com systemd

**Exemplo docker-compose.yml:**
```yaml
version: '3'
services:
  debian-systemd:
    image: debian:12
    privileged: true
    volumes:
      - /sys/fs/cgroup:/sys/fs/cgroup:ro
      - .:/app
    command: /lib/systemd/systemd
```

**Bom para:**
- ✅ systemd-resolved testável
- ✅ Mais leve que VM
- ✅ Automação de testes

**Limitações:**
- ⚠️ Requer modo privilegiado
- ⚠️ NetworkManager pode ter limitações

### Estratégia de Testes Sugerida

```
Fase 1: Desenvolvimento (Docker)
├─ Smoke test Java/JAR
├─ Detecção de distro
└─ Detecção de navegadores

Fase 2: Integração (VM ou container systemd)
├─ Troca de DNS completa
├─ NetworkManager
├─ systemd-resolved
└─ Netplan (Ubuntu)

Fase 3: Validação (VM com GUI)
├─ Big Linux (público brasileiro)
├─ EndeavourOS/CachyOS (Arch-family)
├─ Fedora (RHEL-family)
└─ Kali (caso extremo)

Fase 4: Testes de Campo
└─ Usuários reais nas distros-alvo
```

### Comandos de Teste Rápido

```bash
# Fase 1: Docker smoke test
docker build -t guialar-test .
docker run guialar-test

# Fase 2: VM ou container systemd
# (criar VM com distro-alvo via virt-manager/VirtualBox)
sudo java -jar build/jar/guialar-digital.jar

# Fase 3: Validação
dig @1.1.1.3 malware.testcategory.com  # deve retornar 0.0.0.0
dig @1.1.1.3 nudity.testcategory.com   # deve retornar 0.0.0.0
```

---

## 📝 Nota sobre Branding Futuro

O projeto está atualmente nomeado **GuiaLar Digital**. Existe uma consideração futura de alinhamento ao ecossistema **Big Linux** (distribuição brasileira baseada em Ubuntu) com possíveis nomes como "Big Family" ou "Big Parental". 

**Status:** Apenas em consideração - nenhuma mudança de nome está planejada no momento.

---

---

## ⚠️ ATENÇÃO: DNS-over-HTTPS (DoH) nos Navegadores

### Problema Crítico

**Se o navegador tem DNS Seguro/DoH habilitado, ele IGNORA completamente o DNS do sistema!**

Isso significa que mesmo com o DNS do sistema configurado para Cloudflare Families, o navegador pode usar seu próprio DNS (geralmente o genérico do Cloudflare 1.1.1.1) que **NÃO bloqueia malware nem conteúdo adulto**.

### Solução: Configure DoH para Cloudflare Families

**Endpoint correto:** `https://family.cloudflare-dns.com/dns-query`

**❌ NÃO USE:**
- `https://dns.cloudflare.com/dns-query` (genérico, não bloqueia)
- `https://cloudflare-dns.com/dns-query` (genérico, não bloqueia)

**✅ USE:**
- `https://family.cloudflare-dns.com/dns-query` (bloqueia malware + adulto)

### Como Configurar

#### Firefox
1. Digite `about:preferences#general` na barra de endereços
2. Role até "Configurações de Rede" e clique em "Configurações"
3. Habilite "DNS sobre HTTPS"
4. Selecione "Personalizado"
5. Cole: `https://family.cloudflare-dns.com/dns-query`
6. Clique em OK

#### Chrome / Chromium / Edge / Brave
1. Vá em Configurações → Privacidade e segurança → Segurança
2. Role até "Usar DNS seguro"
3. Habilite e selecione "Personalizado"
4. Cole: `https://family.cloudflare-dns.com/dns-query`
5. Salve as configurações

---

## 🔒 Segurança e Privacidade

### Fluxo de Autorização Obrigatório

O GuiaLar Digital **NUNCA** executa mudanças silenciosamente:

1. ✅ **Exibe plano de ações:** lista clara do que será feito
2. ✅ **Solicita confirmação:** usuário deve autorizar explicitamente
3. ✅ **Eleva privilégios:** usa sudo/polkit conforme necessário
4. ✅ **Faz backup:** salva configurações antes de modificar
5. ✅ **Mostra como reverter:** instruções claras de reversão

### Proteções Implementadas

- **Sem coleta de dados:** O programa não envia informações para servidores externos
- **Código aberto:** Todas as regras e operações são transparentes e auditáveis
- **DNS seguro:** Cloudflare 1.1.1.1 for Families (1.1.1.3 / 1.0.0.3)
  - Bloqueia malware
  - Bloqueia conteúdo adulto (18+)
  - Não coleta histórico de navegação
- **Adblocker:** uBlock Origin / uBlock Origin Lite
  - Proteção contra rastreadores
  - Bloqueio de anúncios invasivos

### Limitações Conhecidas e Contornos

⚠️ **IMPORTANTE:** O DNS do sistema pode ser ignorado por:

#### 1. VPN Ativa
Conexões VPN podem usar DNS próprio e sobrescrever as configurações do sistema.

#### 2. Docker
Containers podem ter configuração DNS separada.

#### 3. DNS-over-HTTPS (DoH) nos Navegadores ⚠️ **CRÍTICO**

**Problema:** Se o navegador usa DNS seguro/DoH, ele **ignora completamente** o DNS do sistema, furando o filtro.

**Solução:** Configure DoH do navegador para usar **Cloudflare Families via DoH:**

**Firefox:**
1. Vá em `about:preferences#general`
2. Role até "Configurações de Rede" → Configurações
3. Habilite "DNS sobre HTTPS"
4. Escolha "Personalizado" e insira:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```
   ⚠️ **NÃO use** `https://cloudflare-dns.com/dns-query` (genérico 1.1.1.1)

**Chrome/Chromium/Edge/Brave:**
1. Vá em Configurações → Privacidade e segurança → Segurança
2. Habilite "Usar DNS seguro"
3. Escolha "Personalizado" e insira:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```
   ⚠️ **NÃO use** `https://dns.cloudflare.com/dns-query` (genérico 1.1.1.1)

**Por que isso é importante:**
- DoH genérico do Cloudflare (1.1.1.1) **NÃO bloqueia** malware nem conteúdo adulto
- Families DoH (`family.cloudflare-dns.com`) mantém a proteção mesmo com DoH ativo

**Recomendação:** Sempre configure DoH do navegador para `family.cloudflare-dns.com/dns-query` para proteção completa.

---

## 📝 Escopo do Projeto

### MVP COMPLETO (Linux):
- ✅ Suporte a Debian, Fedora e Arch Linux
- ✅ Detecção automática de distribuição e gerenciador de rede
- ✅ Configuração de DNS (IPv4 + IPv6) para Cloudflare Families
- ✅ Backup automático antes de modificar configurações
- ✅ Detecção de navegadores Firefox e Chromium (incluindo variantes)
- ✅ Instalação de uBlock Origin (Firefox) e uBlock Origin Lite (Chromium)
- ✅ Plano de ações transparente + autorização explícita
- ✅ Smoke tests (dig / testcategory.com)
- ✅ Instruções de reversão

### ESBOÇO INICIAL (Windows):
- 🔧 Interface para detecção de Windows
- 🔧 Estrutura para PowerShell (Set-DnsClientServerAddress)
- 🔧 Notas sobre DoH no Edge/Chrome
- 🔧 Implementação mínima (não bloqueia MVP Linux)

### FORA DO ESCOPO (por decisão do projeto):
- ❌ macOS
- ❌ Android/iOS
- ❌ Interface gráfica (GUI) - apenas CLI
- ❌ Controle parental invasivo
- ❌ Coleta de histórico de navegação
- ❌ Outras distribuições Linux além da "trindade sagrada"

---

## 🤝 Contribuindo

Este é um projeto acadêmico. Sugestões e melhorias são bem-vindas através de issues e pull requests.

---

## 📄 Licença

Projeto acadêmico desenvolvido para Projetos Integrados I - Uniube (2026/2)

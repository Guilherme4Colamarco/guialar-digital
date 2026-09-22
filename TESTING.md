# Guia de Testes - GuiaLar Digital

## 🧪 Testes do MVP Linux

### Pré-requisitos para Teste Completo

```bash
# Instalar ferramentas de diagnóstico DNS
sudo apt install dnsutils  # Debian/Ubuntu
sudo dnf install bind-utils  # Fedora
sudo pacman -S bind-tools    # Arch
```

### 1. Compilação e Execução

```bash
# Compilar
ant clean
ant jar

# Executar (modo interativo - requer sudo)
sudo java -jar build/jar/guialar-digital.jar
```

### 2. Fluxo de Teste Esperado

1. **Detecção do Sistema**
   - Deve identificar corretamente: Debian, Fedora ou Arch
   - Deve identificar o gerenciador de rede

2. **Detecção de Navegadores**
   - Lista navegadores instalados
   - Identifica corretamente Firefox vs Chromium

3. **Plano de Ações**
   - Exibe lista completa do que será feito
   - Mostra DNS: 1.1.1.3 / 1.0.0.3 (IPv4) e 2606:4700:4700::1113 / 2606:4700:4700::1003 (IPv6)
   - Lista limitações (VPN, Docker, DoH)
   - Confirma privacidade (sem histórico)

4. **Autorização**
   - Pede confirmação explícita (s/N)
   - Verifica privilégios administrativos

5. **Execução**
   - Configura DNS
   - Faz backup
   - Mostra instruções de reversão
   - Configura extensões nos navegadores

### 3. Verificação Pós-Instalação

#### Verificar DNS Configurado

```bash
# Debian/Ubuntu
systemd-resolve --status | grep "DNS Servers"
# ou
resolvectl status
# ou
cat /etc/resolv.conf

# Fedora
nmcli device show | grep DNS

# Arch
resolvectl status
```

Você deve ver `1.1.1.3` e `1.0.0.3` listados.

#### Smoke Tests com dig (URLs Oficiais Cloudflare)

```bash
# Teste 1: Site normal (deve funcionar)
dig @1.1.1.3 example.com
# Esperado: resposta normal com IP válido (ex: 93.184.215.14)

# Teste 2: Malware (deve bloquear)
dig @1.1.1.3 malware.testcategory.com
# Esperado: retorna 0.0.0.0 (bloqueado)

# Teste 3: Conteúdo adulto/nudez (deve bloquear)
dig @1.1.1.3 nudity.testcategory.com
# Esperado: retorna 0.0.0.0 (bloqueado)
```

**Resultado esperado completo:**

```bash
$ dig @1.1.1.3 malware.testcategory.com +short
0.0.0.0

$ dig @1.1.1.3 nudity.testcategory.com +short
0.0.0.0

$ dig @1.1.1.3 example.com +short
93.184.215.14
```

#### Teste com curl

```bash
# Site normal (deve funcionar)
curl -I http://example.com
# Esperado: HTTP 200 OK

# Malware (deve falhar/bloquear)
curl -I http://malware.testcategory.com
# Esperado: falha de conexão (0.0.0.0 não responde)

# Conteúdo adulto/nudez (deve falhar/bloquear)
curl -I http://nudity.testcategory.com
# Esperado: falha de conexão (0.0.0.0 não responde)
```

### 4. Verificar Extensões dos Navegadores

#### Firefox
```bash
# Reinicie o Firefox
firefox &

# Verifique em: about:addons
# Deve aparecer: uBlock Origin instalado
```

#### Chromium/Chrome
```bash
# Abra o navegador
chromium &
# ou
google-chrome &

# Verifique em: chrome://extensions/
# Deve aparecer: uBlock Origin Lite (ou instruções de instalação manual)
```

### 5. Reverter Configurações (se necessário)

O programa exibe instruções de reversão. Exemplo para NetworkManager:

```bash
# Listar conexões
nmcli connection show

# Reverter DNS (substitua 'Ethernet' pelo nome da sua conexão)
nmcli connection modify "Ethernet" ipv4.dns ""
nmcli connection modify "Ethernet" ipv4.ignore-auto-dns no
nmcli connection modify "Ethernet" ipv6.dns ""
nmcli connection modify "Ethernet" ipv6.ignore-auto-dns no
nmcli connection up "Ethernet"
```

---

## 🔍 Diagnóstico de Problemas

### Problema: DNS não está sendo usado

**Verificar se VPN está ativa:**
```bash
ip route | grep tun
# Se retornar algo, VPN pode estar sobrescrevendo DNS
```

**Verificar se navegador usa DoH (CRÍTICO):**

⚠️ **IMPORTANTE:** Se o navegador tem DNS-over-HTTPS habilitado, ele **ignora completamente** o DNS do sistema!

**Firefox:**
1. Vá em `about:config`
2. Buscar `network.trr.mode`:
   - `0` = DoH desabilitado (usa DNS do sistema)
   - `2` = DoH com fallback (prefere DoH)
   - `3` = apenas DoH (ignora DNS do sistema)
3. **Solução:** Configure DoH para usar Cloudflare Families:
   - `about:preferences#general` → Configurações de Rede
   - DNS sobre HTTPS → Personalizado
   - URL: `https://family.cloudflare-dns.com/dns-query`

**Chrome/Chromium/Edge/Brave:**
1. Vá em `chrome://settings/security`
2. Verificar "Usar DNS seguro"
3. **Solução:** Se habilitado, configure para Cloudflare Families:
   - Escolher "Personalizado"
   - URL: `https://family.cloudflare-dns.com/dns-query`

⚠️ **NÃO use o DoH genérico do Cloudflare** (`dns.cloudflare.com`) - ele não bloqueia malware nem conteúdo adulto!

**Verificar se systemd-resolved está rodando:**
```bash
systemctl status systemd-resolved
```

### Problema: Navegadores não detectados

**Verificar instalação:**
```bash
which firefox
which chromium
which google-chrome

# Verificar perfis
ls ~/.mozilla/firefox/
ls ~/.config/chromium/
ls ~/.config/google-chrome/
```

### Problema: Extensões não instaladas

**Firefox:**
- Perfis podem estar em locais diferentes
- Flatpak/Snap usam caminhos específicos

**Chromium:**
- Instalação automática é restrita por segurança
- Geralmente requer instalação manual via Chrome Web Store

---

## 📊 Checklist de Testes

- [ ] Compilação com Ant sem erros
- [ ] Detecção correta da distribuição
- [ ] Detecção de gerenciador de rede
- [ ] Listagem de navegadores instalados
- [ ] Plano de ações exibido corretamente
- [ ] Solicitação de autorização funciona
- [ ] DNS configurado (verificar com dig)
- [ ] Teste de bloqueio de malware funciona
- [ ] Teste de bloqueio de conteúdo adulto funciona
- [ ] Extensões configuradas nos navegadores
- [ ] Instruções de reversão exibidas
- [ ] Nenhum histórico coletado (verificar código)

---

## 🐛 Reportar Problemas

Ao reportar problemas, incluir:

1. Distribuição Linux e versão (`cat /etc/os-release`)
2. Gerenciador de rede (`nmcli --version` ou `systemctl status systemd-resolved`)
3. Navegadores instalados (`which firefox chromium google-chrome`)
4. Saída completa do programa
5. Resultado dos testes com `dig`

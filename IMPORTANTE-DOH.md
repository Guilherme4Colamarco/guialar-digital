# ⚠️ CRÍTICO: Configuração DNS-over-HTTPS (DoH)

## 🔴 Problema

**Se o navegador tem DNS Seguro/DoH habilitado, ele IGNORA completamente o DNS do sistema!**

Mesmo com o GuiaLar configurando corretamente o DNS do sistema para Cloudflare Families (1.1.1.3), se o navegador usa DoH, ele vai usar seu próprio servidor DNS, geralmente o **Cloudflare genérico (1.1.1.1)** que **NÃO bloqueia malware nem conteúdo adulto**.

## ✅ Solução

Configure o DoH do navegador para usar o endpoint **Cloudflare Families**:

```
https://family.cloudflare-dns.com/dns-query
```

## ❌ NÃO USE

Estes endpoints DoH **NÃO bloqueiam** malware/adulto:

- ❌ `https://dns.cloudflare.com/dns-query` (genérico 1.1.1.1)
- ❌ `https://cloudflare-dns.com/dns-query` (genérico)
- ❌ `https://1.1.1.1/dns-query` (genérico)

## ✅ USE

Este endpoint DoH **bloqueia** malware + conteúdo adulto:

- ✅ `https://family.cloudflare-dns.com/dns-query` (Families 1.1.1.3)

---

## 🌍 Como Configurar por Navegador

### Firefox

1. Digite na barra de endereços:
   ```
   about:preferences#general
   ```

2. Role até **"Configurações de Rede"** e clique em **"Configurações"**

3. Habilite **"DNS sobre HTTPS"**

4. Selecione **"Personalizado"**

5. Cole o endpoint:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```

6. Clique em **OK**

### Chrome / Chromium

1. Vá em **Configurações** → **Privacidade e segurança** → **Segurança**

2. Role até **"Usar DNS seguro"**

3. Habilite e selecione **"Personalizado"**

4. Cole o endpoint:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```

5. As configurações são salvas automaticamente

### Microsoft Edge

1. Vá em **Configurações** → **Privacidade, pesquisa e serviços**

2. Role até **"Segurança"** → **"Usar DNS seguro"**

3. Habilite e escolha **"Escolher um provedor de serviços"**

4. Selecione **"Personalizado"**

5. Cole o endpoint:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```

### Brave

1. Vá em **Configurações** → **Privacidade e segurança** → **Segurança**

2. Role até **"Usar DNS seguro"**

3. Habilite e selecione **"Personalizado"**

4. Cole o endpoint:
   ```
   https://family.cloudflare-dns.com/dns-query
   ```

---

## 🧪 Como Testar se Está Funcionando

### Teste 1: Via linha de comando

```bash
# Malware - deve retornar 0.0.0.0 (bloqueado)
dig @1.1.1.3 malware.testcategory.com +short

# Nudity/adulto - deve retornar 0.0.0.0 (bloqueado)
dig @1.1.1.3 nudity.testcategory.com +short

# Site normal - deve retornar IP válido
dig @1.1.1.3 example.com +short
```

### Teste 2: No navegador

Abra estas URLs no navegador (após configurar DoH):

- http://malware.testcategory.com
  - **Esperado:** Falha de conexão ou página de bloqueio

- http://nudity.testcategory.com
  - **Esperado:** Falha de conexão ou página de bloqueio

- http://example.com
  - **Esperado:** Página carrega normalmente

---

## 📊 Diferença Entre os DNSs

| DNS | IP | DoH | Bloqueia Malware? | Bloqueia Adulto? |
|-----|----|----|-------------------|------------------|
| Cloudflare público | 1.1.1.1 | dns.cloudflare.com | ❌ Não | ❌ Não |
| Cloudflare Families (Malware) | 1.1.1.2 | security.cloudflare-dns.com | ✅ Sim | ❌ Não |
| **Cloudflare Families (Malware + Adulto)** | **1.1.1.3** | **family.cloudflare-dns.com** | **✅ Sim** | **✅ Sim** |

O GuiaLar configura o **1.1.1.3** (terceira linha da tabela).

---

## 🔍 Como Verificar se o Navegador Está Usando DoH

### Firefox

1. Digite na barra de endereços:
   ```
   about:config
   ```

2. Aceite o aviso

3. Busque por:
   ```
   network.trr.mode
   ```

4. Valores:
   - `0` = DoH desabilitado (usa DNS do sistema)
   - `2` = DoH com fallback (prefere DoH, usa sistema como backup)
   - `3` = Apenas DoH (ignora completamente DNS do sistema)

5. Se estiver em `2` ou `3`, **você DEVE configurar o DoH para Cloudflare Families**!

### Chrome / Chromium / Edge / Brave

1. Vá em **Configurações** → **Privacidade** → **Segurança**

2. Procure por **"Usar DNS seguro"**

3. Se estiver habilitado, verifique qual provedor está selecionado

4. Se não for "Cloudflare Families" ou "Personalizado" com `family.cloudflare-dns.com`, **você DEVE configurar**!

---

## 📝 Checklist de Configuração

Após executar o GuiaLar Digital, complete esta checklist:

- [ ] DNS do sistema configurado (1.1.1.3 / 1.0.0.3)
- [ ] Testado com `dig @1.1.1.3 malware.testcategory.com` → retorna 0.0.0.0
- [ ] Testado com `dig @1.1.1.3 nudity.testcategory.com` → retorna 0.0.0.0
- [ ] Firefox: DoH configurado para `family.cloudflare-dns.com/dns-query`
- [ ] Chrome: DoH configurado para `family.cloudflare-dns.com/dns-query`
- [ ] Edge: DoH configurado para `family.cloudflare-dns.com/dns-query`
- [ ] Brave: DoH configurado para `family.cloudflare-dns.com/dns-query`
- [ ] Navegadores reiniciados
- [ ] Testado http://malware.testcategory.com no navegador → bloqueado
- [ ] Testado http://nudity.testcategory.com no navegador → bloqueado

---

## 🆘 Troubleshooting

### Problema: Sites maliciosos não estão sendo bloqueados no navegador

**Causa provável:** DoH do navegador está usando DNS genérico.

**Solução:** Siga as instruções de configuração acima para cada navegador.

### Problema: Não consigo acessar nenhum site

**Causa provável:** DoH configurado incorretamente ou DNS do sistema com problema.

**Solução:**

1. Desabilite DoH temporariamente no navegador
2. Teste se consegue acessar sites normalmente
3. Verifique DNS do sistema: `resolvectl status` ou `nmcli device show | grep DNS`
4. Reconfigure DoH com o endpoint correto

### Problema: Quero reverter tudo

**DNS do sistema:**

```bash
# NetworkManager
nmcli connection modify "SUA-CONEXÃO" ipv4.dns ""
nmcli connection modify "SUA-CONEXÃO" ipv4.ignore-auto-dns no
nmcli connection modify "SUA-CONEXÃO" ipv6.dns ""
nmcli connection modify "SUA-CONEXÃO" ipv6.ignore-auto-dns no
nmcli connection up "SUA-CONEXÃO"
```

**DoH do navegador:**

- Firefox: Desabilite "DNS sobre HTTPS" em `about:preferences#general`
- Chrome/Edge/Brave: Desabilite "Usar DNS seguro" nas configurações

---

## 📚 Referências

- [Cloudflare for Families](https://developers.cloudflare.com/1.1.1.1/setup/)
- [Cloudflare DoH](https://developers.cloudflare.com/1.1.1.1/encryption/dns-over-https/)
- [URLs de teste](https://developers.cloudflare.com/1.1.1.1/setup/#test-1111-families)

---

**Este documento é parte do GuiaLar Digital**  
Projeto Integrados I - Uniube - 2026/2

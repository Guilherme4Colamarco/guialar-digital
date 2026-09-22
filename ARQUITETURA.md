# Arquitetura GuiaLar Digital

## 🏗️ Princípio: Core Comum + Adaptadores por OS

O GuiaLar Digital segue uma **arquitetura limpa** que separa:
- **Core comum:** Interfaces e modelos independentes de OS
- **Adaptadores:** Implementações específicas por sistema operacional

**MVP:** Apenas Linux implementado  
**Futuro:** Interfaces definidas, sem código funcional para outros OS

---

## 📦 Camadas

### 1. Domínio (Core Comum)

**Responsabilidade:** Definir interfaces e modelos independentes de OS

```
dominio/
├── sistema/
│   ├── TipoSistema.java           # Enum: LINUX, WINDOWS, MACOS, ANDROID
│   └── SistemaOperacional.java    # Interface base para detecção de OS
├── adaptadores/                    # ⭐ Contratos multi-OS
│   ├── DnsChanger.java            # Interface: configurar()/verificar()/reverter()
│   ├── BrowserDetector.java       # Interface: detectar()/isInstalado()
│   └── ExtensionInstaller.java    # Interface: instalar()/isInstalada()/remover()
├── distro/
│   ├── TipoDistro.java            # Enum: DEBIAN, FEDORA, ARCH
│   └── InfoDistro.java            # Informações da distribuição Linux
├── dns/
│   ├── ServidorDns.java           # Cloudflare Families (1.1.1.3)
│   └── ConfiguracaoDns.java       # Resultado da configuração
├── navegador/
│   ├── TipoNavegador.java         # Enum: FIREFOX, CHROMIUM
│   ├── Navegador.java             # Modelo de navegador
│   └── ExtensaoAdblocker.java     # uBlock Origin / uBlock Origin Lite
├── verificacao/
│   └── ResultadoVerificacao.java  # Resultado smoke tests
└── autorizacao/
    └── PlanoAcao.java             # Plano de ações transparente
```

### 2. Aplicação (Implementações)

**Responsabilidade:** Implementar as interfaces do domínio

```
aplicacao/
├── adaptadores/                    # ⭐ Implementações por OS
│   └── linux/                     # ✅ MVP: ÚNICO OS IMPLEMENTADO
│       ├── LinuxDnsChanger.java
│       ├── LinuxBrowserDetector.java (futuro)
│       └── LinuxExtensionInstaller.java (futuro)
│   ├── macos/                      # 🔧 Futuro: vazio (apenas design)
│   ├── windows/                    # 🔧 Futuro: vazio (apenas design)
│   └── android/                    # 🔧 Futuro: vazio (apenas design)
├── deteccao/
│   ├── DetectorSistemaService.java # Detecta Linux/Windows/Mac
│   ├── DetectorDistroService.java  # Detecta Debian/Fedora/Arch
│   └── DetectorNavegadorService.java
├── dns/
│   ├── ConfiguradorDnsService.java
│   └── ConfiguradorDnsWindowsService.java (esboço)
├── extensao/
│   └── InstaladorExtensaoService.java
├── verificacao/
│   └── VerificacaoDnsService.java
└── autorizacao/
    └── AutorizadorService.java
```

---

## 🔌 Interfaces do Core

### DnsChanger

```java
public interface DnsChanger {
    ConfiguracaoDns configurar(ServidorDns servidor);
    boolean verificar();
    ConfiguracaoDns reverter();
    String getInstrucoesReversao();
}
```

**Implementações:**
- ✅ `LinuxDnsChanger` - NetworkManager/systemd-resolved/Netplan
- 🔧 `MacOsDnsChanger` (futuro) - networksetup/scutil
- 🔧 `WindowsDnsChanger` (futuro) - netsh/PowerShell
- 🔧 `AndroidDnsChanger` (futuro) - Private DNS over TLS

### BrowserDetector

```java
public interface BrowserDetector {
    List<Navegador> detectar();
    boolean isInstalado(String nome);
}
```

**Implementações:**
- ✅ `LinuxBrowserDetector` - PATH/.desktop/perfis ~/.config/
- 🔧 `MacOsBrowserDetector` (futuro) - /Applications/, Homebrew
- 🔧 `WindowsBrowserDetector` (futuro) - Program Files, Registry
- 🔧 `AndroidBrowserDetector` (futuro) - pm list packages

### ExtensionInstaller

```java
public interface ExtensionInstaller {
    boolean instalar(Navegador navegador);
    boolean isInstalada(Navegador navegador);
    boolean remover(Navegador navegador);
}
```

**Implementações:**
- ✅ `LinuxExtensionInstaller` - policies.json (Firefox), managed policies (Chromium)
- 🔧 `MacOsExtensionInstaller` (futuro) - Similar, caminhos diferentes
- 🔧 `WindowsExtensionInstaller` (futuro) - Registry + arquivos
- 🔧 `AndroidExtensionInstaller` (futuro) - Instalação manual via store

---

## 🔍 Diferenças Técnicas por OS

### ✅ Linux (MVP - Implementado)

#### DNS
- **NetworkManager:** `nmcli connection modify`
- **systemd-resolved:** `/etc/systemd/resolved.conf`
- **Netplan:** `/etc/netplan/*.yaml`
- **Fallback:** `/etc/resolv.conf`

#### Browsers
- **Caminhos:** `/usr/bin/`, `/usr/local/bin/`
- **Desktop files:** `/usr/share/applications/`, `~/.local/share/applications/`
- **Perfis:** `~/.mozilla/firefox/`, `~/.config/chromium/`

#### Extensões
- **Firefox:** `~/.mozilla/firefox/distribution/policies.json`
- **Chromium:** Managed policies (geralmente requer instalação manual)

#### Distribuições
- Debian 11+, Ubuntu 20.04+
- Fedora 38+
- Arch Linux, EndeavourOS, CachyOS

---

### 🔧 macOS (Futuro - Design apenas)

#### DNS ⚠️ DIFERENTE DO LINUX
- **networksetup:** `networksetup -setdnsservers Wi-Fi 1.1.1.3 1.0.0.3`
- **scutil:** Configuração mais baixo nível
- **NÃO usa:** NetworkManager, systemd-resolved, Netplan

#### Browsers
- **Aplicações:** `/Applications/Firefox.app/`, `/Applications/Google Chrome.app/`
- **Homebrew:** `/opt/homebrew/Caskroom/`, `/usr/local/Caskroom/`
- **Perfis:** `~/Library/Application Support/Firefox/`, `~/Library/Application Support/Google/Chrome/`

#### Extensões
- **Firefox:** Similar a Linux, mas em `/Applications/Firefox.app/Contents/Resources/distribution/`
- **Chrome:** `~/Library/Application Support/Google/Chrome/External Extensions/`

---

### 🔧 Windows (Futuro - Design apenas)

#### DNS ⚠️ DIFERENTE DO LINUX
- **netsh:** `netsh interface ipv4 set dns "Ethernet" static 1.1.1.3`
- **PowerShell:** `Set-DnsClientServerAddress -InterfaceAlias "Ethernet" -ServerAddresses 1.1.1.3,1.0.0.3`
- **NÃO usa:** nmcli, systemd-resolved

#### Browsers
- **Program Files:** `C:\Program Files\Mozilla Firefox\`, `C:\Program Files\Google\Chrome\`
- **Program Files (x86):** `C:\Program Files (x86)\` (32-bit apps)
- **Registry:** `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\`

#### AppData
- **Local:** `%LOCALAPPDATA%\Mozilla\Firefox\`, `%LOCALAPPDATA%\Google\Chrome\`
- **Roaming:** `%APPDATA%\Mozilla\Firefox\`, `%APPDATA%\Google\Chrome\`

#### Extensões
- **Firefox:** `C:\Program Files\Mozilla Firefox\distribution\policies.json`
- **Chrome:** Registry `HKLM\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist`

---

### 🔧 Android (Futuro - Design apenas)

#### DNS ⚠️ MUITO DIFERENTE DO DESKTOP
- **Private DNS over TLS:** Settings → Network → Private DNS
- **Sem root:** Não muda DNS do sistema programaticamente
- **Usuário deve configurar manualmente:** `dns.family.cloudflare.com`
- **NÃO é possível:** Troca automática como desktop

#### Browsers
- **Packages:** `pm list packages | grep browser`
- **Sem root:** Não há acesso aos perfis dos navegadores
- **App stores:** Google Play Store, F-Droid

#### Extensões
- **Firefox Mobile:** Suporta extensões limitadas via addons.mozilla.org
- **Chrome Mobile:** Não suporta extensões (exceto Kiwi Browser)
- **Instalação:** Sempre manual via store
- **Sem root:** Não é possível forçar instalação

#### Limitações Severas
- Sem root = sem controle do sistema
- Configuração DNS requer ação manual do usuário
- Extensões muito limitadas ou inexistentes
- Abordagem Android deve ser educacional, não automática

---

## 🎯 Fluxo de Seleção de Adaptador (Futuro)

```java
// Pseudocódigo - não implementado no MVP

SistemaOperacional os = DetectorSistemaService.detectar();

DnsChanger dnsChanger = switch (os.getTipo()) {
    case LINUX -> new LinuxDnsChanger(distro);
    case MACOS -> new MacOsDnsChanger();      // futuro
    case WINDOWS -> new WindowsDnsChanger();  // futuro
    case ANDROID -> new AndroidDnsChanger();  // futuro
    default -> throw new UnsupportedOperationException();
};

BrowserDetector browserDetector = switch (os.getTipo()) {
    case LINUX -> new LinuxBrowserDetector();
    case MACOS -> new MacOsBrowserDetector();   // futuro
    case WINDOWS -> new WindowsBrowserDetector(); // futuro
    case ANDROID -> new AndroidBrowserDetector(); // futuro
    default -> throw new UnsupportedOperationException();
};

// ... usar as interfaces
ConfiguracaoDns resultado = dnsChanger.configurar(ServidorDns.getPadrao());
List<Navegador> navegadores = browserDetector.detectar();
```

---

## ✅ Estado Atual (MVP)

| Componente | Linux | macOS | Windows | Android |
|------------|-------|-------|---------|---------|
| **Interface Definida** | ✅ | ✅ | ✅ | ✅ |
| **Implementação** | ✅ | ❌ | ❌ | ❌ |
| **Documentação** | ✅ | ✅ | ✅ | ✅ |
| **Testes** | ✅ | ❌ | ❌ | ❌ |

**Próximos passos (fora do MVP):**
1. Implementar `MacOsDnsChanger`, `MacOsBrowserDetector`, `MacOsExtensionInstaller`
2. Implementar `WindowsDnsChanger`, `WindowsBrowserDetector`, `WindowsExtensionInstaller`
3. Implementar `AndroidDnsChanger` (abordagem educacional, não automática)
4. Criar factory para selecionar adaptador correto por OS
5. Testes em cada plataforma

---

## 📝 Notas de Design

1. **Interfaces primeiro:** Sempre definir a interface no domínio antes de implementar
2. **Um adaptador por OS:** Não misturar lógica de diferentes sistemas
3. **Fallbacks claros:** Cada implementação deve ter fallbacks documentados
4. **Mensagens específicas:** Erros e instruções devem ser específicas do OS
5. **Documentação obrigatória:** Diferenças entre OS devem estar documentadas

---

**Projeto:** GuiaLar Digital  
**Arquitetura:** Core Comum + Adaptadores por OS  
**MVP:** Linux (Debian/Fedora/Arch)  
**Futuro:** macOS, Windows, Android (interfaces definidas)

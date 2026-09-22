# Ambientes de Teste - GuiaLar Digital

Este documento detalha como configurar diferentes ambientes de teste para o GuiaLar Digital.

---

## 🎯 Objetivos por Ambiente

| Ambiente | Objetivo | Fidelidade ao Real | Velocidade | Complexidade |
|----------|----------|-------------------|------------|--------------|
| Docker | Smoke tests rápidos | Baixa | ⚡ Rápido | 🟢 Simples |
| Container systemd | Testes DNS parciais | Média | ⚡ Rápido | 🟡 Média |
| VM (QEMU/libvirt) | Testes completos | Alta | 🐌 Lento | 🔴 Alta |
| VM (VirtualBox) | Testes completos | Alta | 🐌 Lento | 🟡 Média |

---

## 1️⃣ Docker (Smoke Tests)

### Debian 12

```dockerfile
FROM debian:12

# Instalar dependências
RUN apt-get update && apt-get install -y \
    openjdk-17-jdk \
    ant \
    dnsutils \
    firefox-esr \
    chromium \
    curl \
    git

# Copiar código
COPY . /app
WORKDIR /app

# Compilar
RUN ant jar

# Executar testes básicos
CMD ["sh", "-c", "java -jar build/jar/guialar-digital.jar || true"]
```

**Build e teste:**
```bash
docker build -t guialar-debian -f docker/Dockerfile.debian .
docker run -it guialar-debian
```

### Ubuntu 24.04

```dockerfile
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    openjdk-17-jdk \
    ant \
    dnsutils \
    firefox \
    chromium-browser \
    curl

COPY . /app
WORKDIR /app
RUN ant jar

CMD ["java", "-jar", "build/jar/guialar-digital.jar"]
```

### Fedora 40

```dockerfile
FROM fedora:40

RUN dnf install -y \
    java-17-openjdk-devel \
    ant \
    bind-utils \
    firefox \
    chromium

COPY . /app
WORKDIR /app
RUN ant jar

CMD ["java", "-jar", "build/jar/guialar-digital.jar"]
```

### Arch Linux

```dockerfile
FROM archlinux:latest

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm \
    jdk17-openjdk \
    apache-ant \
    bind-tools \
    firefox \
    chromium

COPY . /app
WORKDIR /app
RUN ant jar

CMD ["java", "-jar", "build/jar/guialar-digital.jar"]
```

### docker-compose.yml para todos

```yaml
version: '3'

services:
  debian:
    build:
      context: .
      dockerfile: docker/Dockerfile.debian
    volumes:
      - .:/app

  ubuntu:
    build:
      context: .
      dockerfile: docker/Dockerfile.ubuntu
    volumes:
      - .:/app

  fedora:
    build:
      context: .
      dockerfile: docker/Dockerfile.fedora
    volumes:
      - .:/app

  arch:
    build:
      context: .
      dockerfile: docker/Dockerfile.arch
    volumes:
      - .:/app
```

**Testar todas:**
```bash
docker-compose up --build
```

---

## 2️⃣ Container com systemd

### Debian com systemd

```dockerfile
FROM debian:12

# Instalar systemd e dependências
RUN apt-get update && apt-get install -y \
    systemd \
    systemd-resolved \
    network-manager \
    openjdk-17-jdk \
    ant \
    dnsutils \
    firefox-esr \
    chromium

# Limpar para reduzir tamanho
RUN apt-get clean && rm -rf /var/lib/apt/lists/*

# Copiar código
COPY . /app
WORKDIR /app
RUN ant jar

# systemd como PID 1
CMD ["/lib/systemd/systemd"]
```

**docker-compose com systemd:**
```yaml
version: '3'

services:
  debian-systemd:
    build:
      context: .
      dockerfile: docker/Dockerfile.debian-systemd
    privileged: true
    volumes:
      - /sys/fs/cgroup:/sys/fs/cgroup:ro
      - .:/app
    tmpfs:
      - /run
      - /tmp
```

**Executar testes:**
```bash
docker-compose up -d debian-systemd
docker-compose exec debian-systemd systemctl status systemd-resolved
docker-compose exec debian-systemd bash -c "cd /app && java -jar build/jar/guialar-digital.jar"
```

---

## 3️⃣ VM com QEMU/libvirt

### Criar VM Debian

```bash
# Baixar ISO
wget https://cdimage.debian.org/debian-cd/current/amd64/iso-cd/debian-12.5.0-amd64-netinst.iso

# Criar VM com virt-install
virt-install \
  --name guialar-debian \
  --ram 2048 \
  --vcpus 2 \
  --disk path=/var/lib/libvirt/images/guialar-debian.qcow2,size=20 \
  --os-variant debian12 \
  --network network=default \
  --graphics vnc \
  --cdrom debian-12.5.0-amd64-netinst.iso

# Após instalação, copiar código
scp -r . user@vm-ip:/home/user/guialar-digital
```

### Criar VM Ubuntu (Big Linux base)

```bash
# Baixar ISO Ubuntu
wget https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso

# Criar VM
virt-install \
  --name guialar-ubuntu \
  --ram 4096 \
  --vcpus 2 \
  --disk path=/var/lib/libvirt/images/guialar-ubuntu.qcow2,size=30 \
  --os-variant ubuntu24.04 \
  --network network=default \
  --graphics spice \
  --cdrom ubuntu-24.04-desktop-amd64.iso
```

### Criar VM Fedora

```bash
# Baixar ISO
wget https://download.fedoraproject.org/pub/fedora/linux/releases/40/Workstation/x86_64/iso/Fedora-Workstation-Live-x86_64-40.iso

# Criar VM
virt-install \
  --name guialar-fedora \
  --ram 4096 \
  --vcpus 2 \
  --disk path=/var/lib/libvirt/images/guialar-fedora.qcow2,size=30 \
  --os-variant fedora40 \
  --network network=default \
  --graphics spice \
  --cdrom Fedora-Workstation-Live-x86_64-40.iso
```

### Criar VM EndeavourOS (Arch-family)

```bash
# Baixar ISO
wget https://mirror.alpix.eu/endeavouros/iso/EndeavourOS_Galileo-11-2024.iso

# Criar VM
virt-install \
  --name guialar-endeavour \
  --ram 4096 \
  --vcpus 2 \
  --disk path=/var/lib/libvirt/images/guialar-endeavour.qcow2,size=30 \
  --os-variant archlinux \
  --network network=default \
  --graphics spice \
  --cdrom EndeavourOS_Galileo-11-2024.iso
```

---

## 4️⃣ VirtualBox (Mais Simples)

### Script de criação automatizada

```bash
#!/bin/bash

VM_NAME="guialar-test"
ISO_PATH="/path/to/distro.iso"
DISK_SIZE=20480  # MB

# Criar VM
VBoxManage createvm --name "$VM_NAME" --ostype "Debian_64" --register

# Configurar memória e CPU
VBoxManage modifyvm "$VM_NAME" --memory 2048 --cpus 2

# Criar disco
VBoxManage createhd --filename "$HOME/VirtualBox VMs/$VM_NAME/$VM_NAME.vdi" --size $DISK_SIZE

# Adicionar controladora SATA
VBoxManage storagectl "$VM_NAME" --name "SATA" --add sata --controller IntelAhci

# Anexar disco
VBoxManage storageattach "$VM_NAME" --storagectl "SATA" --port 0 --device 0 --type hdd --medium "$HOME/VirtualBox VMs/$VM_NAME/$VM_NAME.vdi"

# Adicionar controladora IDE para ISO
VBoxManage storagectl "$VM_NAME" --name "IDE" --add ide

# Anexar ISO
VBoxManage storageattach "$VM_NAME" --storagectl "IDE" --port 0 --device 0 --type dvddrive --medium "$ISO_PATH"

# Configurar rede
VBoxManage modifyvm "$VM_NAME" --nic1 nat

# Configurar áudio e vídeo
VBoxManage modifyvm "$VM_NAME" --audio-driver pulse --audioout on
VBoxManage modifyvm "$VM_NAME" --vram 128

# Iniciar VM
VBoxManage startvm "$VM_NAME"
```

---

## 🧪 Matriz de Testes Completa

### Fase 1: Smoke Tests (Docker)

| Distro | Compilação | Detecção Distro | Detecção Navegadores | Status |
|--------|-----------|----------------|---------------------|--------|
| Debian 12 | ✅ | ✅ | ✅ | ✅ |
| Ubuntu 24.04 | ✅ | ✅ | ✅ | ✅ |
| Fedora 40 | 🔧 | 🔧 | 🔧 | 🔧 |
| Arch | 🔧 | 🔧 | 🔧 | 🔧 |

### Fase 2: Testes DNS (VM/Container systemd)

| Distro | NetworkManager | systemd-resolved | Netplan | Troca DNS | Backup | Reversão |
|--------|---------------|-----------------|---------|-----------|--------|----------|
| Debian 12 | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ |
| Ubuntu 24.04 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Fedora 40 | 🔧 | 🔧 | ❌ | 🔧 | 🔧 | 🔧 |
| Arch | 🔧 | 🔧 | ❌ | 🔧 | 🔧 | 🔧 |

### Fase 3: Testes Completos (VM com GUI)

| Distro | DNS | Navegadores | uBlock | DoH | Teste malware | Teste nudity |
|--------|-----|-------------|--------|-----|--------------|--------------|
| Big Linux | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 |
| EndeavourOS | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 |
| CachyOS | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 |
| Kali Linux | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 | 🔧 |

**Legenda:**
- ✅ Testado e funcionando
- 🔧 A testar
- ❌ Não aplicável

---

## 📝 Checklist de Testes por Ambiente

### Docker (Smoke Test)

- [ ] JAR compila sem erros
- [ ] Detecta distribuição corretamente
- [ ] Detecta navegadores instalados
- [ ] Exibe plano de ações
- [ ] Não trava ou crasha

### VM/Container systemd (Testes DNS)

- [ ] NetworkManager é detectado
- [ ] systemd-resolved é detectado
- [ ] DNS é alterado com sucesso
- [ ] Backup é criado
- [ ] `dig @1.1.1.3 malware.testcategory.com` retorna 0.0.0.0
- [ ] `dig @1.1.1.3 nudity.testcategory.com` retorna 0.0.0.0
- [ ] `dig @1.1.1.3 example.com` funciona
- [ ] Instruções de reversão são exibidas
- [ ] Reversão funciona

### VM com GUI (Testes Completos)

- [ ] Tudo do checklist anterior
- [ ] Navegadores Firefox/Chrome são detectados
- [ ] uBlock Origin é configurado (Firefox)
- [ ] uBlock Origin Lite instruções (Chromium)
- [ ] Avisos DoH são exibidos
- [ ] http://malware.testcategory.com é bloqueado no navegador
- [ ] http://nudity.testcategory.com é bloqueado no navegador
- [ ] Sites normais funcionam
- [ ] DoH configurado manualmente funciona

---

## 🎯 Distribuições Prioritárias

### Público Geral Brasileiro

1. **Big Linux** (baseado Ubuntu)
   - ISO: https://www.biglinux.com.br/
   - Por quê: Distribuição nacional, público-alvo natural

2. **Ubuntu 24.04 LTS**
   - Base comum, amplamente utilizada

3. **Linux Mint** (derivado Ubuntu)
   - Muito popular entre iniciantes

### Família Arch User-Friendly

4. **EndeavourOS**
   - ISO: https://endeavouros.com/
   - Por quê: Arch com instalador amigável

5. **CachyOS**
   - ISO: https://cachyos.org/
   - Por quê: Arch otimizado, crescimento recente

### Caso Extremo

6. **Kali Linux**
   - ISO: https://www.kali.org/
   - Por quê: Debian hardened, segurança extrema
   - Nota: É Debian, não Arch

### Enterprise/RHEL-family

7. **Fedora 40 Workstation**
   - Upstream do RHEL, importante para corporativo

---

## 🚀 Script de Teste Automatizado

```bash
#!/bin/bash
# test-all-distros.sh

DISTROS=("debian" "ubuntu" "fedora" "arch")

for distro in "${DISTROS[@]}"; do
    echo "========================================="
    echo "Testando $distro"
    echo "========================================="
    
    docker build -t guialar-$distro -f docker/Dockerfile.$distro .
    
    if docker run guialar-$distro; then
        echo "✅ $distro: PASSOU"
    else
        echo "❌ $distro: FALHOU"
    fi
    
    echo ""
done
```

---

**Projeto:** GuiaLar Digital  
**Nota:** Branding futuro pode incluir "Big Family" / "Big Parental" alinhado ao Big Linux.

# GitHub Actions CI/CD - GuiaLar Digital

## 🔄 Workflows Configurados

### 1. Build and Test (build-and-test.yml)

**Trigger:**
- Push em `main` ou branches `cursor/**`
- Pull requests para `main`

**Jobs:**

#### Build
- ✅ Checkout do código
- ✅ Configura Java 17
- ✅ Instala Apache Ant
- ✅ Compila código (`ant clean jar`)
- ✅ Verifica se JAR foi criado
- ✅ Upload do JAR como artifact

#### Smoke Tests (paralelos)
- ✅ **Debian 12:** Build Docker + Run smoke test
- ✅ **Ubuntu 24.04:** Build Docker + Run smoke test
- ✅ **Fedora 40:** Build Docker + Run smoke test
- ✅ **Arch Linux:** Build Docker + Run smoke test

#### Report
- ✅ Consolida resultados de todos os jobs
- ✅ Exibe status final

**Tempo estimado:** ~10-15 minutos (jobs paralelos)

---

### 2. Quick Check (quick-check.yml)

**Trigger:**
- Push em `main` ou branches `cursor/**`
- Pull requests para `main`

**Jobs:**

#### Quick Build
- ✅ Checkout do código
- ✅ Configura Java 17
- ✅ Instala Apache Ant
- ✅ Compila código
- ✅ Cria JAR
- ✅ Verifica estrutura

**Tempo estimado:** ~2-3 minutos (sem Docker)

---

## 📊 Matriz de Testes

| Workflow | Build | Debian | Ubuntu | Fedora | Arch | Tempo |
|----------|-------|--------|--------|--------|------|-------|
| **Quick Check** | ✅ | - | - | - | - | ~2min |
| **Build and Test** | ✅ | ✅ | ✅ | ✅ | ✅ | ~12min |

---

## 🎯 Quando Cada Workflow Roda

### Quick Check
- Roda em **todos** os pushes e PRs
- Objetivo: Feedback rápido sobre compilação
- Sem Docker: mais rápido

### Build and Test
- Roda em **todos** os pushes e PRs
- Objetivo: Testes completos em todas as distros
- Com Docker: mais confiável

**Ambos rodam em paralelo!**

---

## 📈 Status Badges

No README.md, os badges mostram o status:

```markdown
[![Build and Test](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/build-and-test.yml/badge.svg)](...)
[![Quick Check](https://github.com/Guilherme4Colamarco/guialar-digital/actions/workflows/quick-check.yml/badge.svg)](...)
```

- 🟢 **Passing:** Todos os testes passaram
- 🔴 **Failing:** Algum teste falhou
- 🟡 **Running:** Testes em execução

---

## 🔍 Como Ver os Resultados

1. Vá para: https://github.com/Guilherme4Colamarco/guialar-digital/actions
2. Clique em uma execução (workflow run)
3. Veja os jobs e logs detalhados

**Ou clique nos badges no README!**

---

## 🐛 Troubleshooting

### Workflow não está rodando

**Causas comuns:**
- Branch não está em `main` ou `cursor/**`
- Arquivo `.yml` tem erro de sintaxe
- GitHub Actions desabilitado no repositório

**Solução:**
```bash
# Verificar sintaxe do YAML
yamllint .github/workflows/*.yml

# Testar localmente (com act)
act -l
```

### Smoke test falhou

**Causas comuns:**
- Dockerfile com erro
- Build Ant falhou
- Dependência faltando na imagem Docker

**Solução:**
1. Rodar localmente: `docker build -f docker/Dockerfile.debian .`
2. Ver logs do job no GitHub Actions
3. Corrigir o Dockerfile

### Build demorou muito

**Causas comuns:**
- Docker pull de imagens grandes
- Build paralelo de múltiplas distros

**Normal:** 10-15 minutos é esperado para 4 distros

**Otimização futura:**
- Cache de imagens Docker
- Build apenas da distro principal em PRs
- Testes completos apenas em `main`

---

## 🚀 Melhorias Futuras

### Otimizações
- [ ] Cache do Maven/Ant
- [ ] Cache de imagens Docker
- [ ] Matrix strategy para reduzir duplicação
- [ ] Testes apenas em distro principal para PRs

### Testes Adicionais
- [ ] Testes unitários (quando criados)
- [ ] Verificação de código (linter)
- [ ] Análise de segurança (dependências)
- [ ] Coverage report

### Deploy
- [ ] Release automático com GitHub Releases
- [ ] Upload do JAR como release asset
- [ ] Changelog automático
- [ ] Docker images no GitHub Container Registry

---

## 📝 Exemplo de Output

### Quick Check (sucesso)
```
✅ Compilação OK
✅ JAR gerado
✅ Estrutura OK
🎉 Quick check passou!
```

### Build and Test (sucesso)
```
✅ Build
✅ Debian 12
✅ Ubuntu 24.04
✅ Fedora 40
✅ Arch Linux
🎉 Todos os testes passaram!
```

---

**Projeto:** GuiaLar Digital  
**CI/CD:** GitHub Actions  
**Status:** ✅ Configurado e funcional

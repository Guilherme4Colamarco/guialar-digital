#!/bin/bash
# test-all-distros.sh
# Script de teste automatizado para todas as distribuições

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║      GuiaLar Digital - Smoke Tests em Todas as Distros        ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

DISTROS=("debian" "ubuntu" "fedora" "arch")
RESULTS=()

for distro in "${DISTROS[@]}"; do
    echo "========================================="
    echo "🧪 Testando: $distro"
    echo "========================================="
    echo ""
    
    # Build da imagem
    echo "📦 Building Docker image..."
    if docker build -t guialar-$distro -f docker/Dockerfile.$distro . > /tmp/guialar-build-$distro.log 2>&1; then
        echo "✅ Build successful"
    else
        echo "❌ Build FAILED"
        echo "   Log: /tmp/guialar-build-$distro.log"
        RESULTS+=("$distro:BUILD_FAILED")
        continue
    fi
    
    echo ""
    
    # Run do container
    echo "🚀 Running smoke test..."
    if docker run --rm guialar-$distro > /tmp/guialar-run-$distro.log 2>&1; then
        echo "✅ Smoke test PASSED"
        RESULTS+=("$distro:PASSED")
    else
        echo "❌ Smoke test FAILED"
        echo "   Log: /tmp/guialar-run-$distro.log"
        RESULTS+=("$distro:FAILED")
    fi
    
    echo ""
    echo ""
done

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                      RESULTADOS FINAIS                         ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

PASSED=0
FAILED=0
BUILD_FAILED=0

for result in "${RESULTS[@]}"; do
    distro=$(echo $result | cut -d: -f1)
    status=$(echo $result | cut -d: -f2)
    
    case $status in
        PASSED)
            echo "✅ $distro: PASSOU"
            PASSED=$((PASSED + 1))
            ;;
        FAILED)
            echo "❌ $distro: FALHOU (runtime)"
            FAILED=$((FAILED + 1))
            ;;
        BUILD_FAILED)
            echo "❌ $distro: FALHOU (build)"
            BUILD_FAILED=$((BUILD_FAILED + 1))
            ;;
    esac
done

echo ""
echo "─────────────────────────────────────────────────────────────────"
echo "Total: ${#RESULTS[@]} distros testadas"
echo "Passou: $PASSED"
echo "Falhou (runtime): $FAILED"
echo "Falhou (build): $BUILD_FAILED"
echo "─────────────────────────────────────────────────────────────────"
echo ""

if [ $FAILED -eq 0 ] && [ $BUILD_FAILED -eq 0 ]; then
    echo "✅ TODOS OS TESTES PASSARAM!"
    exit 0
else
    echo "❌ ALGUNS TESTES FALHARAM"
    echo ""
    echo "Verifique os logs em /tmp/guialar-*.log"
    exit 1
fi

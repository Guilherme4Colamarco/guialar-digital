#!/usr/bin/env bash
# Falha se o .exe importar o CRT dinamico (VCRUNTIME*, MSVCP*, api-ms-win-crt-*).
# Windows limpo nao tem o redistribuivel VC++: o exe nao abre (0xC0000135).
# Uso: scripts/check-pe-imports.sh caminho/guialar.exe   (nao executa o exe)
# Teste: CHECK_DUMP=arquivo.txt scripts/check-pe-imports.sh x  (le a lista em vez de rodar a ferramenta)
set -euo pipefail

EXE="${1:?uso: check-pe-imports.sh <exe>}"
RE='(vcruntime[0-9a-z_]*|msvcp[0-9a-z_]*|api-ms-win-crt-[a-z0-9-]+)\.dll'

dump_imports() {
  if [[ -n "${CHECK_DUMP:-}" ]]; then cat "$CHECK_DUMP"; return; fi
  test -f "$EXE" || { echo "::error::exe nao encontrado: $EXE" >&2; return 2; }
  local t
  for t in llvm-objdump objdump; do
    if command -v "$t" >/dev/null 2>&1; then
      echo "ferramenta: $t -p" >&2
      "$t" -p "$EXE"; return
    fi
  done
  local lo="/c/Program Files/LLVM/bin/llvm-objdump.exe"
  if [[ -x "$lo" ]]; then echo "ferramenta: $lo -p" >&2; "$lo" -p "$EXE"; return; fi
  local vsw="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe" inst db
  if [[ -x "$vsw" ]]; then
    inst="$("$vsw" -latest -products '*' -property installationPath | tr -d '\r' | head -n1)"
    db="$(find "$(cygpath -u "$inst")/VC/Tools/MSVC" -name dumpbin.exe -path '*Hostx64/x64*' 2>/dev/null | head -n1)"
    if [[ -n "$db" ]]; then echo "ferramenta: dumpbin /dependents" >&2; "$db" /dependents "$EXE"; return; fi
  fi
  echo "::error::nem llvm-objdump nem dumpbin encontrados para inspecionar $EXE" >&2
  return 2
}

out="$(dump_imports)"
bad="$(printf '%s\n' "$out" | grep -Eio "$RE" | sort -fu || true)"
if [[ -n "$bad" ]]; then
  echo "::error::$EXE importa o CRT dinamico (falha em Windows limpo): $(echo "$bad" | tr '\n' ' ')" >&2
  echo "Confira .cargo/config.toml (+crt-static no x86_64-pc-windows-msvc)." >&2
  exit 1
fi
echo "ok: $EXE sem imports de VCRUNTIME/MSVCP/api-ms-win-crt"

# Segunda guarda: o .exe so pode buscar as DLLs importadas na pasta do sistema
# (/DEPENDENTLOADFLAG:0x800 no build.rs). Sem isso, uma DLL deixada ao lado do
# guialar.exe e carregada no processo elevado.
dump_loadconfig() {
  if [[ -n "${CHECK_LOADCONFIG_DUMP:-}" ]]; then cat "$CHECK_LOADCONFIG_DUMP"; return; fi
  local t
  for t in llvm-readobj "/c/Program Files/LLVM/bin/llvm-readobj.exe"; do
    if command -v "$t" >/dev/null 2>&1 || [[ -x "$t" ]]; then
      echo "ferramenta: $t --coff-load-config" >&2
      "$t" --coff-load-config "$EXE"; return
    fi
  done
  local vsw="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe" inst db
  if [[ -x "$vsw" ]]; then
    inst="$("$vsw" -latest -products '*' -property installationPath | tr -d '\r' | head -n1)"
    db="$(find "$(cygpath -u "$inst")/VC/Tools/MSVC" -name dumpbin.exe -path '*Hostx64/x64*' 2>/dev/null | head -n1)"
    if [[ -n "$db" ]]; then echo "ferramenta: dumpbin /loadconfig" >&2; "$db" /loadconfig "$EXE"; return; fi
  fi
  echo "::error::nem llvm-readobj nem dumpbin encontrados para ler o load config de $EXE" >&2
  return 2
}

lc="$(dump_loadconfig)"
if ! printf '%s\n' "$lc" | grep -Eiq 'DependentLoadFlags?: *0x0*800|0*800 +Dependent Load Flag'; then
  echo "::error::$EXE sem DependentLoadFlags=0x800: DLLs importadas seriam buscadas na pasta do programa" >&2
  echo "Confira o /DEPENDENTLOADFLAG:0x800 no build.rs (alvo msvc)." >&2
  exit 1
fi
echo "ok: $EXE busca as DLLs importadas so na pasta do sistema (DependentLoadFlags=0x800)"

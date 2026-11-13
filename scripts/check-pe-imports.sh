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

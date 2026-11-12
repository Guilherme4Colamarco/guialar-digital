#!/usr/bin/env bash
# E2E headless pela CLI (ADR-029, camada 4). Sem GUI, sem clique.
# Uso: scripts/e2e.sh /caminho/para/guialar
# Roteiro fixo: status -> apply --mode family -> DNS pelo SO -> sonda
#   -> apply de novo (idempotente) -> revert -> DNS igual ao de antes,
#   state.json ausente, sonda volta a resolver.
# Uma linha PASS/FAIL por passo. Exit 0 so se todos passarem; 1 se algum falhar; 2 uso invalido.
# ATENCAO: altera o DNS do computador. Rode so em VM isolada (fedora/void), nunca no host.
set -u

BIN="${1:-}"
PROBE="nudity.testcategory.com"
FAMILY_A="1.1.1.3"
FAMILY_B="1.0.0.3"
STATE="/var/lib/guialar/state.json"  # ADR-031: pasta do sistema (a beta usava ~/.config/guialar)
LOG_DIR="${E2E_LOG_DIR:-$(mktemp -d /tmp/guialar-e2e.XXXXXX)}"
FAILED=0
BASE_DNS=""

if [[ -z "$BIN" || ! -x "$BIN" ]]; then
  echo "uso: $0 /caminho/para/guialar" >&2
  exit 2
fi

pass() { echo "PASS  [$1] $2"; }
fail() { echo "FAIL  [$1] $2"; FAILED=1; }
info() { echo "INFO  $*"; }

# IPv4 de DNS que o SO usa agora (sem o stub 127.0.0.53). Mesmo metodo antes e depois.
os_dns() {
  {
    if command -v resolvectl >/dev/null 2>&1; then resolvectl dns 2>/dev/null; fi
    if command -v nmcli >/dev/null 2>&1; then nmcli -g IP4.DNS device show 2>/dev/null; fi
    if ! command -v resolvectl >/dev/null 2>&1 && ! command -v nmcli >/dev/null 2>&1; then
      grep '^nameserver' /etc/resolv.conf 2>/dev/null
    fi
  } | grep -oE '([0-9]{1,3}\.){3}[0-9]{1,3}' | grep -v '^127\.' | sort -u | paste -sd, -
}

# Resolve pelo resolvedor do SO (getent usa o NSS, como um programa comum faria).
probe() {
  if command -v resolvectl >/dev/null 2>&1; then resolvectl flush-caches >/dev/null 2>&1; fi
  getent ahostsv4 "$PROBE" 2>/dev/null | awk '{print $1}' | sort -u | paste -sd, -
}

dns_is_family() {
  local d
  d="$(os_dns)"
  [[ ",$d," == *",$FAMILY_A,"* && ",$d," == *",$FAMILY_B,"* ]]
}
dns_is_baseline() { [[ -n "$BASE_DNS" && "$(os_dns)" == "$BASE_DNS" ]]; }
probe_is_blocked() { [[ "$(probe)" == "0.0.0.0" ]]; }
probe_resolves() {
  local p
  p="$(probe)"
  [[ -n "$p" && "$p" != "0.0.0.0" ]]
}

# Espera ate a condicao (funcao) virar verdadeira, por N segundos.
wait_until() {
  local cond="$1" secs="$2" deadline
  deadline=$(( $(date +%s) + secs ))
  until "$cond"; do
    if [[ $(date +%s) -ge $deadline ]]; then return 1; fi
    sleep 2
  done
}

# Roda a CLI, guarda a saida e mostra indentada. Retorna o exit code.
run_cli() {
  local name="$1"; shift
  timeout 180 "$BIN" "$@" > "$LOG_DIR/$name.out" 2>&1
  local rc=$?
  echo "$rc" > "$LOG_DIR/$name.rc"
  sed 's/^/      /' "$LOG_DIR/$name.out"
  return $rc
}

rc_of() { cat "$LOG_DIR/$1.rc" 2>/dev/null || echo "?"; }

cleanup() {
  if [[ -e "$STATE" ]]; then
    info "limpeza: state.json ficou; rodando revert para nao deixar o DNS filtrado"
    timeout 180 "$BIN" revert > "$LOG_DIR/cleanup-revert.out" 2>&1 || true
  fi
}
trap cleanup EXIT

info "logs em $LOG_DIR"
info "binario: $BIN ($(sha256sum "$BIN" | cut -c1-16)...)"

# Precondicoes: o teste so faz sentido partindo de um computador sem protecao.
BASE_DNS="$(os_dns)"
info "DNS inicial (SO): ${BASE_DNS:-<vazio>}"
info "sonda inicial: $(probe)"
if [[ -e "$STATE" ]]; then
  fail 0 "precondicao: state.json ja existe antes do teste; use uma VM limpa (ou revert antes)"
  exit 1
fi
if [[ -z "$BASE_DNS" ]]; then
  fail 0 "precondicao: nao consegui ler o DNS inicial pelo SO"
  exit 1
fi
if probe_is_blocked; then
  fail 0 "precondicao: a sonda ja retorna 0.0.0.0 antes do teste (filtro ja ativo)"
  exit 1
fi
# Trava de operacao (temp_dir/guialar-op.lock) de execucao anterior interrompida.
# Liberada se sumiu ou tem mais de 120 s (mesma regra do binario, que tambem checa o PID).
LOCK="${TMPDIR:-/tmp}/guialar-op.lock"
lock_free() {
  [[ ! -e "$LOCK" ]] && return 0
  local age=$(( $(date +%s) - $(stat -c %Y "$LOCK" 2>/dev/null || echo 0) ))
  [[ $age -gt 120 ]]
}
if ! lock_free; then
  info "trava $LOCK recente; esperando expirar (ate 150 s)"
  if ! wait_until lock_free 150; then
    fail 0 "precondicao: trava $LOCK continua recente apos 150 s"
    exit 1
  fi
fi

# 1. status
if run_cli 01-status status; then
  pass 1 "status: exit 0"
else
  fail 1 "status: exit $(rc_of 01-status)"
fi

# 2. apply --mode family
if run_cli 02-apply apply --mode family; then
  pass 2 "apply --mode family: exit 0"
else
  fail 2 "apply --mode family: exit $(rc_of 02-apply)"
fi

# 3. DNS pelo SO
if wait_until dns_is_family 30; then
  pass 3 "DNS do SO = $(os_dns) (contem $FAMILY_A e $FAMILY_B)"
else
  fail 3 "DNS do SO = $(os_dns); esperado conter $FAMILY_A e $FAMILY_B"
fi

# 4. sonda bloqueada
if wait_until probe_is_blocked 30; then
  pass 4 "sonda $PROBE = 0.0.0.0"
else
  fail 4 "sonda $PROBE = $(probe); esperado 0.0.0.0"
fi

# 5. apply de novo (idempotente: nao reescreve o manifesto, DNS continua o da familia)
sha_before="$(sha256sum "$STATE" 2>/dev/null | cut -d' ' -f1)"
if run_cli 05-apply-again apply --mode family; then
  sha_after="$(sha256sum "$STATE" 2>/dev/null | cut -d' ' -f1)"
  if [[ -n "$sha_before" && "$sha_before" == "$sha_after" ]] && wait_until dns_is_family 15; then
    pass 5 "apply de novo: exit 0, state.json inalterado, DNS = $(os_dns)"
  else
    fail 5 "apply de novo: state.json antes=${sha_before:-<ausente>} depois=${sha_after:-<ausente>}; DNS = $(os_dns)"
  fi
else
  fail 5 "apply de novo: exit $(rc_of 05-apply-again)"
fi

# 6. revert
if run_cli 06-revert revert; then
  pass 6 "revert: exit 0"
else
  fail 6 "revert: exit $(rc_of 06-revert)"
fi

# 7. DNS igual ao de antes
if wait_until dns_is_baseline 45; then
  pass 7 "DNS do SO igual ao inicial: $(os_dns)"
else
  fail 7 "DNS do SO = $(os_dns); esperado o inicial: $BASE_DNS"
fi

# 8. state.json ausente
if [[ ! -e "$STATE" ]]; then
  pass 8 "state.json ausente apos revert"
else
  fail 8 "state.json ainda existe: $STATE"
fi

# 9. sonda volta a resolver
if wait_until probe_resolves 30; then
  pass 9 "sonda $PROBE resolve de novo: $(probe)"
else
  fail 9 "sonda $PROBE = '$(probe)'; esperado um IP real (nao vazio, nao 0.0.0.0)"
fi

if [[ $FAILED -eq 0 ]]; then
  echo "RESULTADO: PASS (9/9 passos)"
  exit 0
fi
echo "RESULTADO: FAIL (ver linhas FAIL acima; logs em $LOG_DIR)"
exit 1

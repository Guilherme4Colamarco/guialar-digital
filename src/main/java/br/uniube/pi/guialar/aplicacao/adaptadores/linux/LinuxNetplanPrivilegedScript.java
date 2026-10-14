package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.util.Optional;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Scripts elevados (pkexec) para Netplan: leitura/merge de YAML só como root.
 */
public final class LinuxNetplanPrivilegedScript {

    private static final Pattern STATUS = Pattern.compile("GUAILAR_STATUS=(\\S+)");
    private static final Pattern AVISO = Pattern.compile("GUAILAR_AVISO=(.+)$");

    private LinuxNetplanPrivilegedScript() {
    }

    public static String buildApplyScript(ServidorDns servidor) {
        String guialar = LinuxShellEscape.shSingleQuote(LinuxSystemPaths.NETPLAN_GUAILAR);
        String backup = LinuxShellEscape.shSingleQuote(LinuxSystemPaths.NETPLAN_GUAILAR_BACKUP);
        String py = montarPythonAplicar(servidor);
        return """
            set -e
            GUAILAR=%s
            BACKUP=%s
            if [ -f "$GUAILAR" ]; then cp -a "$GUAILAR" "$BACKUP"; else rm -f "$BACKUP"; fi
            restaurar_guialar() {
              if [ -f "$BACKUP" ]; then mv -f "$BACKUP" "$GUAILAR"; else rm -f "$GUAILAR"; fi
              netplan apply 2>/dev/null || true
            }
            trap restaurar_guialar ERR
            PYTHON_OUT=$(python3 <<'GUAILAR_PY' 2>&1) || true
            echo "$PYTHON_OUT"
            if echo "$PYTHON_OUT" | grep -q 'GUAILAR_STATUS=NO_INTERFACES'; then
              trap - ERR
              rm -f "$BACKUP"
              exit 0
            fi
            if ! echo "$PYTHON_OUT" | grep -q 'GUAILAR_STATUS=OK'; then
              exit 1
            fi
            netplan generate
            netplan apply
            trap - ERR
            rm -f "$BACKUP"
            """.formatted(guialar, backup, py);
    }

    public static String buildRevertScript() {
        String guialar = LinuxShellEscape.shSingleQuote(LinuxSystemPaths.NETPLAN_GUAILAR);
        String backup = LinuxShellEscape.shSingleQuote(LinuxSystemPaths.NETPLAN_GUAILAR_BACKUP);
        return """
            set -e
            GUAILAR=%s
            BACKUP=%s
            if [ -f "$GUAILAR" ]; then cp -a "$GUAILAR" "$BACKUP"; else rm -f "$BACKUP"; fi
            restaurar_guialar() {
              if [ -f "$BACKUP" ]; then mv -f "$BACKUP" "$GUAILAR"; fi
              netplan apply 2>/dev/null || true
            }
            trap restaurar_guialar ERR
            rm -f "$GUAILAR"
            netplan generate
            netplan apply
            trap - ERR
            rm -f "$BACKUP"
            """.formatted(guialar, backup);
    }

    public static Optional<String> parseStatus(String stdout) {
        if (stdout == null) {
            return Optional.empty();
        }
        Matcher m = STATUS.matcher(stdout);
        if (m.find()) {
            return Optional.of(m.group(1));
        }
        return Optional.empty();
    }

    public static Optional<String> parseAviso(String stdout) {
        if (stdout == null) {
            return Optional.empty();
        }
        for (String linha : stdout.split("\\R")) {
            Matcher m = AVISO.matcher(linha.trim());
            if (m.find()) {
                return Optional.of(m.group(1).trim());
            }
        }
        return Optional.empty();
    }

    private static String montarPythonAplicar(ServidorDns servidor) {
        return """
import os, re, sys
NETPLAN_DIR = "/etc/netplan"
GUAILAR = "%s"
ARQUIVO_GUAILAR = os.path.basename(GUAILAR)
DNS4 = [%s, %s]
DNS6 = [%s, %s]

def extrair(yaml):
    secoes = {}
    linhas = yaml.splitlines()
    secao_re = re.compile(r'^(\\s*)(ethernets|wifis):\\s*(#.*)?$')
    chave_re = re.compile(r'^(\\s*)([^\\s#:][^:]*?)\\s*:\\s*(.*)$')
    i = 0
    while i < len(linhas):
        sec = secao_re.match(linhas[i])
        if not sec:
            i += 1
            continue
        indent_secao = len(sec.group(1))
        nome_secao = sec.group(2)
        indent_filho = -1
        atual = None
        dhcp4 = dhcp6 = nameservers = False
        j = i + 1
        while j < len(linhas):
            linha = linhas[j]
            sem = linha.strip()
            if not sem or sem.startswith('#'):
                j += 1
                continue
            indent = len(linhas[j]) - len(linhas[j].lstrip())
            if indent <= indent_secao:
                break
            if indent_filho < 0:
                indent_filho = indent
            ch = chave_re.match(linha)
            if not ch:
                j += 1
                continue
            if indent == indent_filho:
                if atual is not None:
                    adicionar(secoes, nome_secao, atual, dhcp4, dhcp6, nameservers)
                atual = tirar_aspas(ch.group(2).strip())
                dhcp4 = dhcp6 = nameservers = False
            elif atual and indent > indent_filho:
                k = ch.group(2).strip()
                v = re.sub(r'#.*$', '', ch.group(3)).strip().lower()
                verdadeiro = v in ('true', 'yes', 'on')
                if k == 'dhcp4' and verdadeiro:
                    dhcp4 = True
                if k == 'dhcp6' and verdadeiro:
                    dhcp6 = True
                if k == 'nameservers':
                    nameservers = True
            j += 1
        if atual is not None:
            adicionar(secoes, nome_secao, atual, dhcp4, dhcp6, nameservers)
        i = j
    return secoes

def tirar_aspas(s):
    if len(s) >= 2 and ((s[0] == '"' and s[-1] == '"') or (s[0] == "'" and s[-1] == "'")):
        return s[1:-1]
    return s

def adicionar(secoes, secao, iface, dhcp4, dhcp6, nameservers):
    secoes.setdefault(secao, {})
    prev = secoes[secao].get(iface)
    if prev:
        secoes[secao][iface] = (iface, prev[1] or dhcp4, prev[2] or dhcp6, prev[3] or nameservers)
    else:
        secoes[secao][iface] = (iface, dhcp4, dhcp6, nameservers)

secoes = {}
if os.path.isdir(NETPLAN_DIR):
    for nome in sorted(os.listdir(NETPLAN_DIR)):
        if not nome.endswith('.yaml') or nome == ARQUIVO_GUIALAR:
            continue
        path = os.path.join(NETPLAN_DIR, nome)
        try:
            with open(path, 'r', encoding='utf-8') as f:
                mesclar = extrair(f.read())
            for sec, ifaces in mesclar.items():
                for iid, tup in ifaces.items():
                    adicionar(secoes, sec, tup[0], tup[1], tup[2], tup[3])
        except OSError:
            pass

if not secoes:
    print('GUAILAR_STATUS=NO_INTERFACES')
    sys.exit(0)

removidas = []
for sec in list(secoes.keys()):
    for iid in list(secoes[sec].keys()):
        if secoes[sec][iid][3]:
            removidas.append(iid)
            del secoes[sec][iid]
    if not secoes[sec]:
        del secoes[sec]

if removidas:
    print('GUAILAR_AVISO=' + ','.join(removidas))

if not secoes:
    print('GUAILAR_STATUS=NO_INTERFACES')
    sys.exit(0)

lines = [
    '# Configurado por GuiaLar Digital',
    '# Só acrescenta DNS às interfaces já definidas nos outros arquivos.',
    'network:',
    '  version: 2',
]
for sec, ifaces in secoes.items():
    if not ifaces:
        continue
    lines.append(f'  {sec}:')
    for iid, (_, dhcp4, dhcp6, _) in ifaces.items():
        esc = iid.replace('"', '\\\\"')
        lines.append(f'    "{esc}":')
        lines.append('      nameservers:')
        lines.append('        addresses:')
        for d in DNS4:
            lines.append(f'          - {d}')
        for d in DNS6:
            lines.append(f'          - "{d}"')
        if dhcp4:
            lines.append('      dhcp4-overrides:')
            lines.append('        use-dns: false')
        if dhcp6:
            lines.append('      dhcp6-overrides:')
            lines.append('        use-dns: false')

with open(GUAILAR, 'w', encoding='utf-8') as out:
    out.write('\\n'.join(lines) + '\\n')
os.chmod(GUAILAR, 0o600)
print('GUAILAR_STATUS=OK')
""".formatted(
            LinuxSystemPaths.NETPLAN_GUAILAR.replace("\\", "\\\\"),
            shPyString(servidor.getPrimario()),
            shPyString(servidor.getSecundario()),
            shPyString(servidor.getPrimarioIpv6()),
            shPyString(servidor.getSecundarioIpv6())
        );
    }

    private static String shPyString(String s) {
        return "'" + s.replace("'", "'\"'\"'") + "'";
    }
}

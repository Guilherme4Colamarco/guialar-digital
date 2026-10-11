package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Parsing e geração do {@code 99-guialar-dns.yaml} sem editar outros YAMLs do Netplan.
 */
public final class LinuxNetplanSupport {

    private static final Pattern SECAO_NETPLAN = Pattern.compile("^(\\s*)(ethernets|wifis):\\s*(#.*)?$");
    private static final Pattern CHAVE_YAML = Pattern.compile("^(\\s*)([^\\s#:][^:]*?)\\s*:\\s*(.*)$");

    private static final String MARCADOR_ARQUIVO = "#GUIALAR-ARQUIVO ";

    private LinuxNetplanSupport() {
    }

    public record InterfaceNetplan(String id, boolean dhcp4, boolean dhcp6, boolean temNameservers) {
        InterfaceNetplan mesclar(InterfaceNetplan outra) {
            return new InterfaceNetplan(id, dhcp4 || outra.dhcp4, dhcp6 || outra.dhcp6,
                temNameservers || outra.temNameservers);
        }
    }

    public static List<Path> listarYamlsExistentes(Path netplanDir, String arquivoGuialar) throws IOException {
        List<Path> yamls = new ArrayList<>();
        if (!Files.isDirectory(netplanDir)) {
            return yamls;
        }
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(netplanDir, "*.yaml")) {
            for (Path p : stream) {
                if (!p.getFileName().toString().equals(arquivoGuialar)) {
                    yamls.add(p);
                }
            }
        }
        yamls.sort(null);
        return yamls;
    }

    public static Map<String, Map<String, InterfaceNetplan>> lerInterfaces(Collection<String> yamls) {
        Map<String, Map<String, InterfaceNetplan>> secoes = new LinkedHashMap<>();
        for (String yaml : yamls) {
            mesclar(secoes, extrairInterfacesNetplan(yaml));
        }
        return secoes;
    }

    /** Script para pkexec que imprime os YAMLs que o usuário comum não consegue ler (ex.: 600 root). */
    public static String scriptLerArquivos(List<Path> arquivos) {
        StringBuilder sb = new StringBuilder("set -e\n");
        for (int i = 0; i < arquivos.size(); i++) {
            sb.append("printf '%s\\n' '").append(MARCADOR_ARQUIVO).append(i).append("'\n");
            sb.append("cat -- ").append(LinuxShellEscape.shSingleQuote(arquivos.get(i).toString())).append('\n');
            sb.append("echo\n");
        }
        return sb.toString();
    }

    public static List<String> separarSaidaLeitura(String stdout, int esperados) throws IOException {
        List<StringBuilder> partes = new ArrayList<>();
        for (String linha : (stdout == null ? "" : stdout).split("\\R", -1)) {
            if (linha.equals(MARCADOR_ARQUIVO + partes.size())) {
                partes.add(new StringBuilder());
            } else if (!partes.isEmpty()) {
                partes.get(partes.size() - 1).append(linha).append('\n');
            }
        }
        if (partes.size() != esperados) {
            throw new IOException("leitura dos arquivos do Netplan incompleta (" + partes.size()
                + " de " + esperados + ")");
        }
        return partes.stream().map(StringBuilder::toString).toList();
    }

    public static List<String> separarComDnsProprio(Map<String, Map<String, InterfaceNetplan>> secoes) {
        List<String> removidas = new ArrayList<>();
        for (var it = secoes.entrySet().iterator(); it.hasNext(); ) {
            Map<String, InterfaceNetplan> ifaces = it.next().getValue();
            ifaces.values().removeIf(i -> {
                if (i.temNameservers()) {
                    removidas.add(i.id());
                    return true;
                }
                return false;
            });
            if (ifaces.isEmpty()) {
                it.remove();
            }
        }
        return removidas;
    }

    private static void mesclar(Map<String, Map<String, InterfaceNetplan>> destino,
                                Map<String, Map<String, InterfaceNetplan>> origem) {
        for (var secao : origem.entrySet()) {
            Map<String, InterfaceNetplan> alvo = destino.computeIfAbsent(secao.getKey(), k -> new LinkedHashMap<>());
            for (InterfaceNetplan i : secao.getValue().values()) {
                alvo.merge(i.id(), i, InterfaceNetplan::mesclar);
            }
        }
    }

    public static Map<String, Map<String, InterfaceNetplan>> extrairInterfacesNetplan(String yaml) {
        Map<String, Map<String, InterfaceNetplan>> resultado = new LinkedHashMap<>();
        String[] linhas = yaml.split("\\R");
        for (int i = 0; i < linhas.length; i++) {
            Matcher secao = SECAO_NETPLAN.matcher(linhas[i]);
            if (!secao.matches()) {
                continue;
            }
            int indentSecao = secao.group(1).length();
            String nomeSecao = secao.group(2);
            int indentFilho = -1;
            String atual = null;
            boolean dhcp4 = false, dhcp6 = false, nameservers = false;

            int j = i + 1;
            for (; j < linhas.length; j++) {
                String linha = linhas[j];
                String semEspaco = linha.strip();
                if (semEspaco.isEmpty() || semEspaco.startsWith("#")) {
                    continue;
                }
                int indent = linha.length() - linha.stripLeading().length();
                if (indent <= indentSecao) {
                    break;
                }
                if (indentFilho < 0) {
                    indentFilho = indent;
                }
                Matcher chave = CHAVE_YAML.matcher(linha);
                if (indent == indentFilho && chave.matches()) {
                    if (atual != null) {
                        adicionar(resultado, nomeSecao, new InterfaceNetplan(atual, dhcp4, dhcp6, nameservers));
                    }
                    atual = tirarAspas(chave.group(2).strip());
                    dhcp4 = false;
                    dhcp6 = false;
                    nameservers = false;
                } else if (atual != null && indent > indentFilho && chave.matches()) {
                    String k = chave.group(2).strip();
                    String v = chave.group(3).replaceAll("#.*$", "").strip().toLowerCase();
                    boolean verdadeiro = v.equals("true") || v.equals("yes") || v.equals("on");
                    if (k.equals("dhcp4") && verdadeiro) dhcp4 = true;
                    if (k.equals("dhcp6") && verdadeiro) dhcp6 = true;
                    if (k.equals("nameservers")) nameservers = true;
                }
            }
            if (atual != null) {
                adicionar(resultado, nomeSecao, new InterfaceNetplan(atual, dhcp4, dhcp6, nameservers));
            }
            i = j - 1;
        }
        return resultado;
    }

    private static void adicionar(Map<String, Map<String, InterfaceNetplan>> resultado,
                                  String secao, InterfaceNetplan iface) {
        resultado.computeIfAbsent(secao, k -> new LinkedHashMap<>())
            .merge(iface.id(), iface, InterfaceNetplan::mesclar);
    }

    private static String tirarAspas(String s) {
        if (s.length() >= 2 && (s.startsWith("\"") && s.endsWith("\"") || s.startsWith("'") && s.endsWith("'"))) {
            return s.substring(1, s.length() - 1);
        }
        return s;
    }

    public static String gerarYamlGuialar(Map<String, Map<String, InterfaceNetplan>> secoes, ServidorDns servidor) {
        String arquivo = Path.of(LinuxSystemPaths.NETPLAN_GUAILAR).getFileName().toString();
        StringBuilder sb = new StringBuilder();
        sb.append("# Configurado por GuiaLar Digital\n");
        sb.append("# Só acrescenta DNS às interfaces já definidas nos outros arquivos.\n");
        sb.append("# Para reverter: use Desfazer proteção no GuiaLar (remove ").append(arquivo).append(").\n");
        sb.append("network:\n");
        sb.append("  version: 2\n");
        for (var secao : secoes.entrySet()) {
            if (secao.getValue().isEmpty()) {
                continue;
            }
            sb.append("  ").append(secao.getKey()).append(":\n");
            for (InterfaceNetplan i : secao.getValue().values()) {
                sb.append("    \"").append(i.id().replace("\"", "\\\"")).append("\":\n");
                sb.append("      nameservers:\n");
                sb.append("        addresses:\n");
                sb.append("          - ").append(servidor.getPrimario()).append('\n');
                sb.append("          - ").append(servidor.getSecundario()).append('\n');
                sb.append("          - \"").append(servidor.getPrimarioIpv6()).append("\"\n");
                sb.append("          - \"").append(servidor.getSecundarioIpv6()).append("\"\n");
                if (i.dhcp4()) {
                    sb.append("      dhcp4-overrides:\n");
                    sb.append("        use-dns: false\n");
                }
                if (i.dhcp6()) {
                    sb.append("      dhcp6-overrides:\n");
                    sb.append("        use-dns: false\n");
                }
            }
        }
        return sb.toString();
    }
}

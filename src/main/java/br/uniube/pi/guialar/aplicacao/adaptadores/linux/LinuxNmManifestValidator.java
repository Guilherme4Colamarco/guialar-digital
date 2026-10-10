package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.net.Inet6Address;
import java.net.InetAddress;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * Valida entradas do manifesto NM antes de montar scripts pkexec (anti-injeção).
 */
public class LinuxNmManifestValidator {

    private static final Pattern UUID_PATTERN = Pattern.compile(
        "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$");

    private final LinuxCommandRunner runner;

    public LinuxNmManifestValidator(LinuxCommandRunner runner) {
        this.runner = runner;
    }

    public record Resultado(boolean valido, String mensagem, List<LinuxNmConexaoEstado> conexoes) {
        public static Resultado ok(List<LinuxNmConexaoEstado> conexoes) {
            return new Resultado(true, "", conexoes);
        }

        public static Resultado erro(String mensagem) {
            return new Resultado(false, mensagem, List.of());
        }
    }

    public Resultado validarParaRevert(List<LinuxNmConexaoEstado> doManifesto) {
        if (doManifesto == null || doManifesto.isEmpty()) {
            return Resultado.erro("O manifesto não lista conexões para desfazer.");
        }
        Map<String, String> uuidParaNome = listarConexoesNm();
        List<LinuxNmConexaoEstado> saneadas = new ArrayList<>();
        for (LinuxNmConexaoEstado bruta : doManifesto) {
            String uuid = resolverUuid(bruta, uuidParaNome);
            if (uuid == null) {
                return Resultado.erro(
                    "O manifesto parece alterado ou desatualizado (conexão \""
                        + resumir(bruta.nome()) + "\" não existe no NetworkManager). "
                        + "Desfazer cancelado por segurança.");
            }
            String v4dns = validarListaDns(bruta.ipv4Dns(), true);
            if (v4dns == null) {
                return Resultado.erro("Valor inválido de DNS IPv4 no manifesto. Desfazer cancelado por segurança.");
            }
            String v6dns = validarListaDns(bruta.ipv6Dns(), false);
            if (v6dns == null) {
                return Resultado.erro("Valor inválido de DNS IPv6 no manifesto. Desfazer cancelado por segurança.");
            }
            String ign4 = validarIgnoreAutoDns(bruta.ipv4IgnoreAutoDns());
            String ign6 = validarIgnoreAutoDns(bruta.ipv6IgnoreAutoDns());
            if (ign4 == null || ign6 == null) {
                return Resultado.erro(
                    "Valor inválido de ignore-auto-dns no manifesto. Desfazer cancelado por segurança.");
            }
            String nome = uuidParaNome.get(uuid);
            saneadas.add(new LinuxNmConexaoEstado(uuid, nome, v4dns, v6dns, ign4, ign6));
        }
        return Resultado.ok(saneadas);
    }

    /**
     * Valida estado lido ao vivo do nmcli (antes de aplicar).
     */
    public Resultado validarEstadoAoVivo(LinuxNmConexaoEstado estado) {
        return validarParaRevert(List.of(estado));
    }

    private String resolverUuid(LinuxNmConexaoEstado bruta, Map<String, String> uuidParaNome) {
        if (bruta.uuid() != null && !bruta.uuid().isBlank()) {
            String u = bruta.uuid().trim();
            if (!UUID_PATTERN.matcher(u).matches() || !uuidParaNome.containsKey(u)) {
                return null;
            }
            return u;
        }
        String nome = bruta.nome();
        if (nome == null || nome.isBlank() || nome.length() > 256) {
            return null;
        }
        for (Map.Entry<String, String> e : uuidParaNome.entrySet()) {
            if (e.getValue().equals(nome)) {
                return e.getKey();
            }
        }
        return null;
    }

    private Map<String, String> listarConexoesNm() {
        Map<String, String> mapa = new HashMap<>();
        try {
            LinuxCommandResult r = runner.executar(false, "nmcli", "-t", "-f", "UUID,NAME", "connection", "show");
            if (!r.sucesso() || r.stdout() == null) {
                return mapa;
            }
            for (String linha : r.stdout().split("\n")) {
                if (linha.isBlank()) {
                    continue;
                }
                List<String> p = LinuxNmcliTerseParser.splitFields(linha.trim(), 2);
                if (p.size() < 2) {
                    continue;
                }
                String uuid = p.get(0).trim();
                String nome = p.get(1).trim();
                if (UUID_PATTERN.matcher(uuid).matches()) {
                    mapa.put(uuid, nome);
                }
            }
        } catch (Exception ignored) {
        }
        return mapa;
    }

    static String validarIgnoreAutoDns(String valor) {
        if (valor == null) {
            return "no";
        }
        String v = valor.trim().toLowerCase();
        if ("yes".equals(v) || "no".equals(v)) {
            return v;
        }
        return null;
    }

    static String validarListaDns(String valor, boolean ipv4) {
        if (valor == null || valor.isBlank()) {
            return "";
        }
        String[] partes = valor.trim().split("\\s+");
        List<String> ok = new ArrayList<>();
        for (String parte : partes) {
            if (!parte.isEmpty() && isEnderecoValido(parte, ipv4)) {
                ok.add(parte);
            } else {
                return null;
            }
        }
        return String.join(" ", ok);
    }

    private static boolean isEnderecoValido(String ip, boolean ipv4) {
        try {
            InetAddress addr = InetAddress.getByName(ip);
            if (ipv4) {
                return addr instanceof java.net.Inet4Address;
            }
            return addr instanceof Inet6Address;
        } catch (Exception e) {
            return false;
        }
    }

    private static String resumir(String s) {
        if (s == null) {
            return "?";
        }
        return s.length() > 40 ? s.substring(0, 37) + "..." : s;
    }
}

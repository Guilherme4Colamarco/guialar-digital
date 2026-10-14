package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.net.Inet6Address;
import java.net.InetAddress;
import java.util.regex.Pattern;

/**
 * Valida manifesto de DNS antes de montar scripts PowerShell elevados.
 */
public final class WindowsDnsManifestValidator {

    private static final Pattern ALIAS_PERMITIDO =
        Pattern.compile("^[\\w .()\\-]{1,128}$");
    private static final int INDEX_MIN = 1;
    private static final int INDEX_MAX = 256;

    private WindowsDnsManifestValidator() {
    }

    /**
     * @return mensagem de erro em português, ou {@code null} se válido
     */
    public static String validar(WindowsDnsManifestStore.ManifestoDns manifesto) {
        if (manifesto == null || manifesto.adaptadores() == null || manifesto.adaptadores().isEmpty()) {
            return "manifesto vazio";
        }
        for (WindowsDnsManifestStore.EstadoAdaptador a : manifesto.adaptadores().values()) {
            String erro = validarAdaptador(a);
            if (erro != null) {
                return erro;
            }
        }
        return null;
    }

    private static String validarAdaptador(WindowsDnsManifestStore.EstadoAdaptador a) {
        if (a == null) {
            return "entrada de adaptador ausente";
        }
        if (a.interfaceIndex() < INDEX_MIN || a.interfaceIndex() > INDEX_MAX) {
            return "índice de interface inválido";
        }
        if (!textoSeguro(a.alias()) || !ALIAS_PERMITIDO.matcher(a.alias()).matches()) {
            return "alias de interface inválido";
        }
        for (String ip : a.ipv4Anterior()) {
            if (!ipv4Valido(ip)) {
                return "IPv4 anterior inválido no manifesto";
            }
        }
        for (String ip : a.ipv6Anterior()) {
            if (!ipv6Valido(ip)) {
                return "IPv6 anterior inválido no manifesto";
            }
        }
        return null;
    }

    private static boolean textoSeguro(String s) {
        if (s == null || s.isEmpty()) {
            return false;
        }
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c < 32 || c == 127) {
                return false;
            }
        }
        return true;
    }

    private static boolean ipv4Valido(String ip) {
        if (!textoSeguro(ip)) {
            return false;
        }
        try {
            InetAddress addr = InetAddress.getByName(ip);
            return addr instanceof java.net.Inet4Address && ip.equals(addr.getHostAddress());
        } catch (Exception e) {
            return false;
        }
    }

    private static boolean ipv6Valido(String ip) {
        if (!textoSeguro(ip)) {
            return false;
        }
        try {
            InetAddress addr = InetAddress.getByName(ip);
            return addr instanceof Inet6Address;
        } catch (Exception e) {
            return false;
        }
    }
}

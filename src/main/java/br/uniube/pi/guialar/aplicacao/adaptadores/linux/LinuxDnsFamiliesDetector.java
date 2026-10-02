package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.util.regex.Pattern;

/**
 * Detecta IPs Cloudflare Families sem falso positivo (ex.: 1.1.1.30).
 */
public final class LinuxDnsFamiliesDetector {

    private static final Pattern IPV4_FAMILIES = Pattern.compile(
        "\\b(1\\.1\\.1\\.3|1\\.0\\.0\\.3)\\b");
    private static final Pattern IPV6_FAMILIES = Pattern.compile(
        "\\b(2606:4700:4700::1113|2606:4700:4700::1003)\\b");

    private LinuxDnsFamiliesDetector() {
    }

    public static boolean textoContemFamilies(String texto) {
        if (texto == null || texto.isBlank()) {
            return false;
        }
        return IPV4_FAMILIES.matcher(texto).find() || IPV6_FAMILIES.matcher(texto).find();
    }
}

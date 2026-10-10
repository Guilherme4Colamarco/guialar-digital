package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Parser de saída {@code nmcli -t} com suporte a {@code \:} escapado.
 */
public final class LinuxNmcliTerseParser {

    private LinuxNmcliTerseParser() {
    }

    /**
     * Divide uma linha -t em campos (máximo {@code maxFields}).
     */
    public static List<String> splitFields(String linha, int maxFields) {
        List<String> campos = new ArrayList<>();
        if (linha == null || linha.isEmpty()) {
            return campos;
        }
        StringBuilder atual = new StringBuilder();
        int count = 1;
        for (int i = 0; i < linha.length(); i++) {
            char c = linha.charAt(i);
            if (c == '\\' && i + 1 < linha.length() && linha.charAt(i + 1) == ':') {
                atual.append(':');
                i++;
                continue;
            }
            if (c == ':' && count < maxFields) {
                campos.add(atual.toString());
                atual.setLength(0);
                count++;
                continue;
            }
            atual.append(c);
        }
        campos.add(atual.toString());
        return campos;
    }

    /**
     * Parse de {@code nmcli -t -f NAME,TYPE,DEVICE connection show --active}.
     */
    public static List<ConexaoAtiva> parseConexoesAtivas(String stdout) {
        List<ConexaoAtiva> lista = new ArrayList<>();
        if (stdout == null) {
            return lista;
        }
        for (String linha : stdout.split("\n")) {
            if (linha.isBlank()) {
                continue;
            }
            List<String> p = splitFields(linha.trim(), 3);
            if (p.size() < 3) {
                continue;
            }
            lista.add(new ConexaoAtiva(p.get(0), p.get(1), p.get(2)));
        }
        return lista;
    }

    public record ConexaoAtiva(String nome, String tipo, String device) {
        public boolean isVpnOuTunel() {
            String t = tipo.toLowerCase();
            String d = device.toLowerCase();
            return t.contains("vpn") || t.contains("wireguard") || t.contains("tun")
                || d.contains("tun") || d.contains("wg") || d.contains("vpn");
        }

        public boolean isRedeFisicaOuWifi() {
            String t = tipo.toLowerCase();
            return t.contains("ethernet") || t.contains("802-3") || t.contains("wireless")
                || t.contains("wifi") || t.contains("802-11");
        }
    }

    /**
     * Parse {@code nmcli -t -f ipv4.dns,ipv6.dns,ipv4.ignore-auto-dns,ipv6.ignore-auto-dns connection show NAME}.
     */
    public static Map<String, String> parseCamposDns(String stdout) {
        Map<String, String> mapa = new LinkedHashMap<>();
        if (stdout == null) {
            return mapa;
        }
        for (String linha : stdout.split("\n")) {
            int idx = linha.indexOf(':');
            if (idx <= 0) {
                continue;
            }
            mapa.put(linha.substring(0, idx), linha.substring(idx + 1));
        }
        return mapa;
    }
}

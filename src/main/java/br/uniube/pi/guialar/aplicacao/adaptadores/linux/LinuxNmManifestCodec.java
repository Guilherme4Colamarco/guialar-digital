package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.util.ArrayList;
import java.util.List;
import java.util.Properties;

public final class LinuxNmManifestCodec {

    public static final String KEY_NM_COUNT = "nm.count";

    private LinuxNmManifestCodec() {
    }

    public static void gravarConexoes(Properties props, List<LinuxNmConexaoEstado> conexoes) {
        props.setProperty(KEY_NM_COUNT, String.valueOf(conexoes.size()));
        for (int i = 0; i < conexoes.size(); i++) {
            LinuxNmConexaoEstado c = conexoes.get(i);
            String p = "nm." + i + ".";
            props.setProperty(p + "name", c.nome());
            props.setProperty(p + "ipv4.dns", nullToEmpty(c.ipv4Dns()));
            props.setProperty(p + "ipv6.dns", nullToEmpty(c.ipv6Dns()));
            props.setProperty(p + "ipv4.ignore-auto-dns", nullToEmpty(c.ipv4IgnoreAutoDns()));
            props.setProperty(p + "ipv6.ignore-auto-dns", nullToEmpty(c.ipv6IgnoreAutoDns()));
        }
    }

    public static List<LinuxNmConexaoEstado> lerConexoes(Properties props) {
        List<LinuxNmConexaoEstado> lista = new ArrayList<>();
        int count = Integer.parseInt(props.getProperty(KEY_NM_COUNT, "0"));
        for (int i = 0; i < count; i++) {
            String p = "nm." + i + ".";
            lista.add(new LinuxNmConexaoEstado(
                props.getProperty(p + "name", ""),
                props.getProperty(p + "ipv4.dns", ""),
                props.getProperty(p + "ipv6.dns", ""),
                props.getProperty(p + "ipv4.ignore-auto-dns", "no"),
                props.getProperty(p + "ipv6.ignore-auto-dns", "no")
            ));
        }
        return lista;
    }

    private static String nullToEmpty(String s) {
        return s == null ? "" : s;
    }
}

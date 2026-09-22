package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.stream.Collectors;

/**
 * Manifesto do que o GuiaLar alterou no DNS Windows, para Desfazer.
 * Formato: properties em %LOCALAPPDATA%\\GuiaLar\\dns-manifest.properties
 */
public class WindowsDnsManifestStore {

    public static final String KEY_TIMESTAMP = "timestamp";
    public static final String KEY_METODO = "metodo";
    public static final String KEY_ADAPTERS = "adapters";
    public static final String KEY_SERVIDOR = "servidor.alvo";
    public static final String PREFIX_IFACE = "iface.";

    private final Path arquivo;

    public WindowsDnsManifestStore() {
        this(WindowsPaths.manifestoDns());
    }

    public WindowsDnsManifestStore(Path arquivo) {
        this.arquivo = arquivo;
    }

    public Path getArquivo() {
        return arquivo;
    }

    public boolean existe() {
        try {
            return Files.isRegularFile(arquivo) && Files.size(arquivo) > 0;
        } catch (IOException e) {
            return false;
        }
    }

    public void salvar(ManifestoDns manifesto) throws IOException {
        Files.createDirectories(arquivo.getParent());
        Properties props = new Properties();
        props.setProperty(KEY_TIMESTAMP, manifesto.timestamp());
        props.setProperty(KEY_METODO, manifesto.metodo());
        props.setProperty(KEY_SERVIDOR, manifesto.servidorAlvo());
        props.setProperty(KEY_ADAPTERS, String.join("|", manifesto.adaptadores().keySet()));

        for (Map.Entry<String, EstadoAdaptador> e : manifesto.adaptadores().entrySet()) {
            String alias = e.getKey();
            EstadoAdaptador est = e.getValue();
            String base = PREFIX_IFACE + escapar(alias) + ".";
            props.setProperty(base + "index", String.valueOf(est.interfaceIndex()));
            props.setProperty(base + "ipv4", String.join(",", est.ipv4Anterior()));
            props.setProperty(base + "ipv6", String.join(",", est.ipv6Anterior()));
            props.setProperty(base + "dhcp", String.valueOf(est.eraDhcp()));
        }

        try (OutputStream out = Files.newOutputStream(arquivo)) {
            props.store(out, "GuiaLar Digital - manifesto DNS Windows (nao editar manualmente)");
        }
    }

    public ManifestoDns carregar() throws IOException {
        if (!existe()) {
            return null;
        }
        Properties props = new Properties();
        try (InputStream in = Files.newInputStream(arquivo)) {
            props.load(in);
        }

        Map<String, EstadoAdaptador> adaptadores = new LinkedHashMap<>();
        String adaptersCsv = props.getProperty(KEY_ADAPTERS, "");
        if (!adaptersCsv.isBlank()) {
            for (String alias : adaptersCsv.split("\\|")) {
                if (alias.isBlank()) {
                    continue;
                }
                String base = PREFIX_IFACE + escapar(alias) + ".";
                int index = parseIntSafe(props.getProperty(base + "index", "-1"), -1);
                List<String> ipv4 = splitCsv(props.getProperty(base + "ipv4", ""));
                List<String> ipv6 = splitCsv(props.getProperty(base + "ipv6", ""));
                boolean dhcp = Boolean.parseBoolean(props.getProperty(base + "dhcp", "true"));
                adaptadores.put(alias, new EstadoAdaptador(alias, index, ipv4, ipv6, dhcp));
            }
        }

        return new ManifestoDns(
            props.getProperty(KEY_TIMESTAMP, Instant.now().toString()),
            props.getProperty(KEY_METODO, "Set-DnsClientServerAddress"),
            props.getProperty(KEY_SERVIDOR, ""),
            adaptadores
        );
    }

    public void limpar() throws IOException {
        Files.deleteIfExists(arquivo);
    }

    private static String escapar(String alias) {
        return alias.replace(' ', '_').replace('=', '_').replace(':', '_');
    }

    private static List<String> splitCsv(String csv) {
        if (csv == null || csv.isBlank()) {
            return List.of();
        }
        return Arrays.stream(csv.split(","))
            .map(String::trim)
            .filter(s -> !s.isEmpty())
            .collect(Collectors.toList());
    }

    private static int parseIntSafe(String s, int def) {
        try {
            return Integer.parseInt(s.trim());
        } catch (Exception e) {
            return def;
        }
    }

    /**
     * Snapshot de um adaptador antes da alteração.
     */
    public record EstadoAdaptador(
        String alias,
        int interfaceIndex,
        List<String> ipv4Anterior,
        List<String> ipv6Anterior,
        boolean eraDhcp
    ) {
        public EstadoAdaptador {
            ipv4Anterior = ipv4Anterior == null ? List.of() : List.copyOf(ipv4Anterior);
            ipv6Anterior = ipv6Anterior == null ? List.of() : List.copyOf(ipv6Anterior);
        }
    }

    /**
     * Manifesto completo.
     */
    public record ManifestoDns(
        String timestamp,
        String metodo,
        String servidorAlvo,
        Map<String, EstadoAdaptador> adaptadores
    ) {
        public ManifestoDns {
            adaptadores = adaptadores == null
                ? Map.of()
                : Collections.unmodifiableMap(new LinkedHashMap<>(adaptadores));
        }

        public static ManifestoDns criar(String metodo, String servidorAlvo,
                                         Map<String, EstadoAdaptador> adaptadores) {
            return new ManifestoDns(Instant.now().toString(), metodo, servidorAlvo, adaptadores);
        }

        public List<String> aliases() {
            return new ArrayList<>(adaptadores.keySet());
        }
    }
}

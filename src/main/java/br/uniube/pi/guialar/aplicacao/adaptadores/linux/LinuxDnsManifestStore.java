package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.Properties;

/**
 * Manifesto do DNS Linux em ~/.guialar/ (usuário real, nunca /root).
 */
public class LinuxDnsManifestStore {

    public static final String KEY_TIMESTAMP = "timestamp";
    public static final String KEY_METODO = "metodo";
    public static final String KEY_CONEXAO = "conexao_nm";
    public static final String KEY_BACKUP_NM = "backup_nm";
    public static final String KEY_ARQUIVO_NETPLAN = "arquivo_netplan";
    public static final String KEY_NIXOS_SNIPPET = "nixos_snippet";
    public static final String KEY_MODO = "modo";

    public static final String MODO_APLICADO = "aplicado";
    public static final String MODO_AGUARDANDO_NIXOS = "aguardando_nixos";

    private final Path arquivo;

    public LinuxDnsManifestStore() {
        this(Path.of(System.getProperty("user.home"), ".guialar", "dns-manifest.properties"));
    }

    public LinuxDnsManifestStore(Path arquivo) {
        this.arquivo = arquivo;
    }

    public Path getArquivo() {
        return arquivo;
    }

    public boolean existe() {
        return Files.isRegularFile(arquivo);
    }

    public void salvar(Properties props) throws IOException {
        Files.createDirectories(arquivo.getParent());
        props.setProperty(KEY_TIMESTAMP, Instant.now().toString());
        try (OutputStream out = Files.newOutputStream(arquivo)) {
            props.store(out, "GuiaLar Digital - manifesto DNS Linux");
        }
    }

    public Properties carregar() throws IOException {
        Properties props = new Properties();
        try (InputStream in = Files.newInputStream(arquivo)) {
            props.load(in);
        }
        return props;
    }

    public void remover() throws IOException {
        Files.deleteIfExists(arquivo);
    }
}

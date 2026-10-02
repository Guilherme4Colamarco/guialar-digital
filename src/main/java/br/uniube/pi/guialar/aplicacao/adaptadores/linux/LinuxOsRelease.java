package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Leitura de /etc/os-release (ou conteúdo injetado em testes).
 */
public class LinuxOsRelease {

    private final Map<String, String> valores;

    public LinuxOsRelease(Map<String, String> valores) {
        this.valores = valores;
    }

    public static LinuxOsRelease ler(Path arquivo) throws IOException {
        if (arquivo == null || !Files.exists(arquivo)) {
            return new LinuxOsRelease(Map.of());
        }
        return parse(Files.readAllLines(arquivo));
    }

    public static LinuxOsRelease parse(List<String> linhas) {
        Map<String, String> mapa = new HashMap<>();
        for (String linha : linhas) {
            int idx = linha.indexOf('=');
            if (idx <= 0) {
                continue;
            }
            String chave = linha.substring(0, idx).trim();
            String valor = linha.substring(idx + 1).trim().replaceAll("^\"|\"$", "");
            mapa.put(chave, valor);
        }
        return new LinuxOsRelease(mapa);
    }

    public String getId() {
        return valores.getOrDefault("ID", "");
    }

    public boolean isNixOs() {
        return "nixos".equalsIgnoreCase(getId());
    }
}

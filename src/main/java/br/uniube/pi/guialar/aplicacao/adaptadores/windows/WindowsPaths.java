package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.nio.file.Path;

/**
 * Caminhos graváveis pelo usuário no Windows (%LOCALAPPDATA%\\GuiaLar).
 * Em não-Windows, usa ~/.guialar para testes/local.
 */
public final class WindowsPaths {

    private WindowsPaths() {
    }

    public static Path diretorioDados() {
        String localAppData = System.getenv("LOCALAPPDATA");
        if (localAppData != null && !localAppData.isBlank()) {
            return Path.of(localAppData, "GuiaLar");
        }
        return Path.of(System.getProperty("user.home"), ".guialar");
    }

    public static Path manifestoDns() {
        return diretorioDados().resolve("dns-manifest.properties");
    }

    public static Path logDiagnostico() {
        return diretorioDados().resolve("ultimo-diagnostico.txt");
    }
}

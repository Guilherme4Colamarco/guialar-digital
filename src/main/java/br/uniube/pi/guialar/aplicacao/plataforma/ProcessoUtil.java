package br.uniube.pi.guialar.aplicacao.plataforma;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;

/**
 * Utilitário para executar comandos externos e capturar a saída.
 *
 * Usado pelos adaptadores por sistema operacional (ex.: PowerShell no Windows,
 * {@code id}/{@code dig} no Linux).
 */
public final class ProcessoUtil {

    private ProcessoUtil() {
    }

    /** Resultado de um comando: código de saída e saída combinada (stdout+stderr). */
    public static final class Resultado {
        private final int codigo;
        private final String saida;

        public Resultado(int codigo, String saida) {
            this.codigo = codigo;
            this.saida = saida;
        }

        public int getCodigo() {
            return codigo;
        }

        public String getSaida() {
            return saida;
        }

        public boolean ok() {
            return codigo == 0;
        }
    }

    /** Executa um comando e retorna o resultado. Nunca lança exceção. */
    public static Resultado executar(String... comando) {
        try {
            Process processo = new ProcessBuilder(comando)
                .redirectErrorStream(true)
                .start();

            StringBuilder sb = new StringBuilder();
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(processo.getInputStream(), StandardCharsets.UTF_8))) {
                String linha;
                while ((linha = reader.readLine()) != null) {
                    sb.append(linha).append('\n');
                }
            }

            int codigo = processo.waitFor();
            return new Resultado(codigo, sb.toString().trim());
        } catch (Exception e) {
            return new Resultado(-1, e.getMessage() == null ? "" : e.getMessage());
        }
    }

    /** Executa um script PowerShell (somente relevante no Windows). */
    public static Resultado powershell(String script) {
        return executar(
            "powershell.exe",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy", "Bypass",
            "-Command", script);
    }
}

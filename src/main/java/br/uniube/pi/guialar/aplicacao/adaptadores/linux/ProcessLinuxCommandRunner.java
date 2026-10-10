package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * Executa comandos reais. Elevação via {@code pkexec} (um script por operação).
 */
public class ProcessLinuxCommandRunner implements LinuxCommandRunner {

    @Override
    public LinuxCommandResult executar(boolean privilegiado, String... comando)
            throws IOException, InterruptedException {
        if (comando == null || comando.length == 0) {
            throw new IllegalArgumentException("comando vazio");
        }
        if (privilegiado) {
            throw new IllegalArgumentException("Use executarScriptPrivilegiado para operações elevadas");
        }
        Process process = new ProcessBuilder(List.of(comando))
            .redirectErrorStream(false)
            .start();

        String stdout = lerStream(process.getInputStream());
        String stderr = lerStream(process.getErrorStream());
        int code = process.waitFor();
        return new LinuxCommandResult(code, stdout, stderr, false);
    }

    @Override
    public LinuxCommandResult executarScriptPrivilegiado(String script) throws IOException, InterruptedException {
        if (!comandoDisponivel("pkexec")) {
            return new LinuxCommandResult(127, "",
                "pkexec não encontrado. Instale policykit-1 (pkexec) ou execute em ambiente com polkit.",
                false);
        }
        Process process = new ProcessBuilder("pkexec", "/bin/bash", "-s")
            .redirectErrorStream(false)
            .start();
        try (OutputStream out = process.getOutputStream()) {
            out.write(script.getBytes(StandardCharsets.UTF_8));
        }
        String stdout = lerStream(process.getInputStream());
        String stderr = lerStream(process.getErrorStream());
        int code = process.waitFor();
        return new LinuxCommandResult(code, stdout, stderr, true);
    }

    @Override
    public boolean comandoDisponivel(String comando) {
        try {
            Process p = new ProcessBuilder("which", comando).redirectErrorStream(true).start();
            return p.waitFor() == 0;
        } catch (Exception e) {
            return false;
        }
    }

    private static String lerStream(java.io.InputStream in) throws IOException {
        StringBuilder sb = new StringBuilder();
        try (BufferedReader reader = new BufferedReader(new InputStreamReader(in, StandardCharsets.UTF_8))) {
            String linha;
            while ((linha = reader.readLine()) != null) {
                if (sb.length() > 0) {
                    sb.append('\n');
                }
                sb.append(linha);
            }
        }
        return sb.toString();
    }
}

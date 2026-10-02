package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * Executa comandos reais. Elevação via {@code pkexec}.
 */
public class ProcessLinuxCommandRunner implements LinuxCommandRunner {

    @Override
    public LinuxCommandResult executar(boolean privilegiado, String... comando)
            throws IOException, InterruptedException {
        if (comando == null || comando.length == 0) {
            throw new IllegalArgumentException("comando vazio");
        }
        List<String> linhaComando = new ArrayList<>();
        boolean viaPkexec = false;
        if (privilegiado) {
            if (!comandoDisponivel("pkexec")) {
                return new LinuxCommandResult(127, "",
                    "pkexec não encontrado. Instale policykit-1 (pkexec) ou execute em ambiente com polkit.",
                    false);
            }
            linhaComando.add("pkexec");
            viaPkexec = true;
        }
        for (String parte : comando) {
            linhaComando.add(parte);
        }

        Process process = new ProcessBuilder(linhaComando)
            .redirectErrorStream(false)
            .start();

        String stdout = lerStream(process.getInputStream());
        String stderr = lerStream(process.getErrorStream());
        int code = process.waitFor();
        return new LinuxCommandResult(code, stdout, stderr, viaPkexec);
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

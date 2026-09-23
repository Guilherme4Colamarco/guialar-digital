package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.Charset;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

/**
 * Executor real via {@code powershell.exe} / {@code pwsh}.
 * Em sistemas não-Windows devolve {@link PowerShellResult#indisponivel}.
 */
public class ProcessPowerShellExecutor implements PowerShellExecutor {

    private static final long TIMEOUT_SEGUNDOS = 60;

    @Override
    public PowerShellResult executar(String script, boolean elevado) {
        if (!isWindows()) {
            return PowerShellResult.indisponivel(
                "PowerShell do Windows não está disponível neste sistema.");
        }

        try {
            List<String> comando = new ArrayList<>();
            String shell = localizarPowerShell();
            if (shell == null) {
                return PowerShellResult.indisponivel(
                    "powershell.exe / pwsh não encontrado no PATH.");
            }

            if (elevado) {
                // Start-Process -Verb RunAs exige interação UAC; o script elevado
                // grava saída em arquivo temporário para o processo pai ler.
                String escaped = script.replace("'", "''");
                String elevScript = """
                    $ErrorActionPreference = 'Stop'
                    $outFile = [System.IO.Path]::GetTempFileName()
                    $errFile = [System.IO.Path]::GetTempFileName()
                    $arg = "-NoProfile -ExecutionPolicy Bypass -Command `"& { %s } *>$outFile; if (-not $?) { $_ | Out-File $errFile }`"
                    try {
                      $p = Start-Process -FilePath '%s' -Verb RunAs -ArgumentList $arg -Wait -PassThru
                      $code = $p.ExitCode
                      if (Test-Path $outFile) { Get-Content -Raw $outFile }
                      if (Test-Path $errFile) { Get-Content -Raw $errFile | Write-Error }
                      exit $code
                    } catch {
                      Write-Output "GUIA_LAR_UAC_DENIED:$($_.Exception.Message)"
                      exit 1223
                    }
                    """.formatted(escaped, shell.replace("'", "''"));
                comando.add(shell);
                comando.add("-NoProfile");
                comando.add("-ExecutionPolicy");
                comando.add("Bypass");
                comando.add("-Command");
                comando.add(elevScript);
            } else {
                comando.add(shell);
                comando.add("-NoProfile");
                comando.add("-ExecutionPolicy");
                comando.add("Bypass");
                comando.add("-Command");
                comando.add(script);
            }

            ProcessBuilder pb = new ProcessBuilder(comando);
            pb.redirectErrorStream(false);
            Process process = pb.start();

            String stdout = lerStream(process.getInputStream());
            String stderr = lerStream(process.getErrorStream());

            boolean terminou = process.waitFor(TIMEOUT_SEGUNDOS, TimeUnit.SECONDS);
            if (!terminou) {
                process.destroyForcibly();
                return new PowerShellResult(-1, stdout, "Timeout ao executar PowerShell", false, false);
            }

            int code = process.exitValue();
            String combinado = (stdout + "\n" + stderr).toLowerCase();
            if (code == 1223 || combinado.contains("guia_lar_uac_denied")
                    || combinado.contains("canceled by the user")
                    || combinado.contains("cancelado pelo usuário")
                    || combinado.contains("the operation was canceled")) {
                return PowerShellResult.elevacaoNegada(
                    "Elevação UAC negada ou cancelada pelo usuário.");
            }

            return new PowerShellResult(code, stdout.trim(), stderr.trim());
        } catch (Exception e) {
            return new PowerShellResult(-1, "", e.getMessage());
        }
    }

    private static boolean isWindows() {
        String os = System.getProperty("os.name", "").toLowerCase();
        return os.contains("win");
    }

    private static String localizarPowerShell() {
        String[] candidatos = {"powershell.exe", "powershell", "pwsh.exe", "pwsh"};
        for (String c : candidatos) {
            try {
                Process p = new ProcessBuilder(c, "-NoProfile", "-Command", "exit 0")
                    .redirectErrorStream(true)
                    .start();
                if (p.waitFor(10, TimeUnit.SECONDS) && p.exitValue() == 0) {
                    return c;
                }
            } catch (Exception ignored) {
                // tenta próximo
            }
        }
        return null;
    }

    private static String lerStream(java.io.InputStream in) throws java.io.IOException {
        Charset charset = Charset.defaultCharset();
        if (StandardCharsets.UTF_8.equals(charset)) {
            charset = StandardCharsets.UTF_8;
        }
        StringBuilder sb = new StringBuilder();
        try (BufferedReader br = new BufferedReader(new InputStreamReader(in, charset))) {
            String line;
            while ((line = br.readLine()) != null) {
                if (sb.length() > 0) {
                    sb.append('\n');
                }
                sb.append(line);
            }
        }
        return sb.toString();
    }
}

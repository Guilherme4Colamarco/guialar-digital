package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

/**
 * Resultado de uma invocação PowerShell.
 */
public final class PowerShellResult {

    private final int exitCode;
    private final String stdout;
    private final String stderr;
    private final boolean elevacaoNegada;
    private final boolean indisponivel;

    public PowerShellResult(int exitCode, String stdout, String stderr) {
        this(exitCode, stdout, stderr, false, false);
    }

    public PowerShellResult(int exitCode, String stdout, String stderr,
                            boolean elevacaoNegada, boolean indisponivel) {
        this.exitCode = exitCode;
        this.stdout = stdout == null ? "" : stdout;
        this.stderr = stderr == null ? "" : stderr;
        this.elevacaoNegada = elevacaoNegada;
        this.indisponivel = indisponivel;
    }

    public static PowerShellResult indisponivel(String motivo) {
        return new PowerShellResult(-1, "", motivo, false, true);
    }

    public static PowerShellResult elevacaoNegada(String detalhe) {
        return new PowerShellResult(1223, "", detalhe, true, false);
    }

    public int getExitCode() {
        return exitCode;
    }

    public String getStdout() {
        return stdout;
    }

    public String getStderr() {
        return stderr;
    }

    public boolean isOk() {
        return !indisponivel && !elevacaoNegada && exitCode == 0;
    }

    public boolean isElevacaoNegada() {
        return elevacaoNegada;
    }

    public boolean isIndisponivel() {
        return indisponivel;
    }

    public String getSaidaCombinada() {
        if (stdout.isBlank()) {
            return stderr;
        }
        if (stderr.isBlank()) {
            return stdout;
        }
        return stdout + "\n" + stderr;
    }
}

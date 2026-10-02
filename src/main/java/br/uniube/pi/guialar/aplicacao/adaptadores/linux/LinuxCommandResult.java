package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Resultado de execução de comando no Linux (normal ou via pkexec).
 */
public record LinuxCommandResult(int exitCode, String stdout, String stderr, boolean viaPkexec) {

    public boolean sucesso() {
        return exitCode == 0;
    }

    public String saidaCombinada() {
        if (stderr == null || stderr.isBlank()) {
            return stdout == null ? "" : stdout;
        }
        if (stdout == null || stdout.isBlank()) {
            return stderr;
        }
        return stdout + "\n" + stderr;
    }
}

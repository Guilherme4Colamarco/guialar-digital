package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Escape seguro para literais entre aspas simples em shell POSIX.
 */
public final class LinuxShellEscape {

    private LinuxShellEscape() {
    }

    public static String shSingleQuote(String valor) {
        if (valor == null) {
            return "''";
        }
        return "'" + valor.replace("'", "'\"'\"'") + "'";
    }
}

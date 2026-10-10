package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Mensagens pt-BR para falhas de pkexec/polkit.
 */
public final class LinuxPkexecMensagens {

    private LinuxPkexecMensagens() {
    }

    public static String mensagemUsuario(int exitCode) {
        if (exitCode == 126) {
            return "Você cancelou a senha de administrador. Nada foi alterado.";
        }
        if (exitCode == 127) {
            return "Não foi possível pedir autorização de administrador "
                + "(pkexec indisponível ou sem agente gráfico do polkit). Nada foi alterado.";
        }
        return null;
    }

    public static boolean isCancelamentoOuSemAgente(int exitCode) {
        return exitCode == 126 || exitCode == 127;
    }
}

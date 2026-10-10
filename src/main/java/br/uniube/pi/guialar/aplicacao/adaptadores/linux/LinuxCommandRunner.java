package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.IOException;

/**
 * Executa comandos no sistema. {@code privilegiado=true} usa pkexec (polkit).
 */
public interface LinuxCommandRunner {

    LinuxCommandResult executar(boolean privilegiado, String... comando) throws IOException, InterruptedException;

    /**
     * Uma única elevação pkexec executando o script em {@code bash -s} (stdin).
     */
    LinuxCommandResult executarScriptPrivilegiado(String script) throws IOException, InterruptedException;

    boolean comandoDisponivel(String comando);
}

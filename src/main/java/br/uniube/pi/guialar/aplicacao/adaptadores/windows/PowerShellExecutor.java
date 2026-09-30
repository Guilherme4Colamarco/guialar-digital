package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

/**
 * Contrato para executar comandos PowerShell.
 * Permite mocks nos testes em CI Linux sem PowerShell real.
 */
public interface PowerShellExecutor {

    /**
     * Executa um script PowerShell e devolve stdout/stderr/exit code.
     *
     * @param script corpo do script (sem -Command wrapper)
     * @param elevado se true, tenta elevação UAC (apenas em Windows real)
     */
    PowerShellResult executar(String script, boolean elevado);

    default PowerShellResult executar(String script) {
        return executar(script, false);
    }
}

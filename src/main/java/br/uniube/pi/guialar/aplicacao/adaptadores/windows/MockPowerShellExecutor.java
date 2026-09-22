package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.util.ArrayList;
import java.util.List;
import java.util.function.BiFunction;

/**
 * Executor PowerShell mockável para testes em CI Linux.
 */
public class MockPowerShellExecutor implements PowerShellExecutor {

    private final List<String> scriptsRecebidos = new ArrayList<>();
    private BiFunction<String, Boolean, PowerShellResult> handler;

    public MockPowerShellExecutor() {
        this.handler = (script, elev) -> new PowerShellResult(0, "", "");
    }

    public MockPowerShellExecutor(BiFunction<String, Boolean, PowerShellResult> handler) {
        this.handler = handler;
    }

    public void setHandler(BiFunction<String, Boolean, PowerShellResult> handler) {
        this.handler = handler;
    }

    public List<String> getScriptsRecebidos() {
        return scriptsRecebidos;
    }

    @Override
    public PowerShellResult executar(String script, boolean elevado) {
        scriptsRecebidos.add((elevado ? "[ELEV] " : "") + script);
        return handler.apply(script, elevado);
    }
}

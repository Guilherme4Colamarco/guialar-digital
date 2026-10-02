package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;

/**
 * Runner fake para testes: registra chamadas e devolve respostas configuráveis.
 */
public class FakeLinuxCommandRunner implements LinuxCommandRunner {

    public record Chamada(boolean privilegiado, List<String> comando, String script) {
        public String chave() {
            if (script != null) {
                return "script:" + script.lines().findFirst().orElse("");
            }
            return String.join(" ", comando);
        }
    }

    private final List<Chamada> historico = new ArrayList<>();
    private final List<String> scriptsPrivilegiados = new ArrayList<>();
    private final Map<String, LinuxCommandResult> respostas = new HashMap<>();
    private final Map<String, Boolean> comandosDisponiveis = new HashMap<>();
    private BiFunction<Chamada, FakeLinuxCommandRunner, LinuxCommandResult> fallback;
    private LinuxCommandResult resultadoScriptPadrao = new LinuxCommandResult(0, "", "", true);

    public FakeLinuxCommandRunner() {
        comandosDisponiveis.put("pkexec", true);
        comandosDisponiveis.put("nmcli", true);
        comandosDisponiveis.put("which", true);
        comandosDisponiveis.put("systemctl", true);
        comandosDisponiveis.put("resolvectl", true);
        comandosDisponiveis.put("getent", true);
    }

    public List<Chamada> getHistorico() {
        return historico;
    }

    public List<String> getScriptsPrivilegiados() {
        return scriptsPrivilegiados;
    }

    public void responder(String prefixoComando, LinuxCommandResult resultado) {
        respostas.put(prefixoComando, resultado);
    }

    public void setComandoDisponivel(String comando, boolean disponivel) {
        comandosDisponiveis.put(comando, disponivel);
    }

    public void setFallback(BiFunction<Chamada, FakeLinuxCommandRunner, LinuxCommandResult> fallback) {
        this.fallback = fallback;
    }

    public void setResultadoScriptPadrao(LinuxCommandResult resultadoScriptPadrao) {
        this.resultadoScriptPadrao = resultadoScriptPadrao;
    }

    @Override
    public LinuxCommandResult executar(boolean privilegiado, String... comando) {
        if (privilegiado) {
            throw new IllegalStateException("Use executarScriptPrivilegiado nos testes");
        }
        Chamada chamada = new Chamada(false, List.of(comando), null);
        historico.add(chamada);
        return resolver(chamada);
    }

    @Override
    public LinuxCommandResult executarScriptPrivilegiado(String script) {
        scriptsPrivilegiados.add(script);
        Chamada chamada = new Chamada(true, List.of("pkexec", "/bin/bash", "-s"), script);
        historico.add(chamada);
        for (Map.Entry<String, LinuxCommandResult> e : respostas.entrySet()) {
            if (script.contains(e.getKey())) {
                LinuxCommandResult base = e.getValue();
                return new LinuxCommandResult(base.exitCode(), base.stdout(), base.stderr(), true);
            }
        }
        if (fallback != null) {
            return fallback.apply(chamada, this);
        }
        return resultadoScriptPadrao;
    }

    private LinuxCommandResult resolver(Chamada chamada) {
        String chave = chamada.chave();
        for (Map.Entry<String, LinuxCommandResult> e : respostas.entrySet()) {
            if (chave.startsWith(e.getKey()) || chave.contains(e.getKey())) {
                LinuxCommandResult base = e.getValue();
                return new LinuxCommandResult(base.exitCode(), base.stdout(), base.stderr(), false);
            }
        }
        if (fallback != null) {
            return fallback.apply(chamada, this);
        }
        return new LinuxCommandResult(0, "", "", false);
    }

    @Override
    public boolean comandoDisponivel(String comando) {
        return comandosDisponiveis.getOrDefault(comando, false);
    }
}

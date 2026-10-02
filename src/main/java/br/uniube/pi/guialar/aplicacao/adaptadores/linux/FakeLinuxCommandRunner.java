package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;

/**
 * Runner fake para testes: registra chamadas e devolve respostas configuráveis.
 */
public class FakeLinuxCommandRunner implements LinuxCommandRunner {

    public record Chamada(boolean privilegiado, List<String> comando) {
        public String chave() {
            return String.join(" ", comando);
        }
    }

    private final List<Chamada> historico = new ArrayList<>();
    private final Map<String, LinuxCommandResult> respostas = new HashMap<>();
    private final Map<String, Boolean> comandosDisponiveis = new HashMap<>();
    private BiFunction<Chamada, FakeLinuxCommandRunner, LinuxCommandResult> fallback;

    public FakeLinuxCommandRunner() {
        comandosDisponiveis.put("pkexec", true);
        comandosDisponiveis.put("nmcli", true);
        comandosDisponiveis.put("which", true);
    }

    public List<Chamada> getHistorico() {
        return historico;
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

    public void simularOsRelease(Path arquivo, String conteudo) throws IOException {
        Files.writeString(arquivo, conteudo);
    }

    @Override
    public LinuxCommandResult executar(boolean privilegiado, String... comando) {
        Chamada chamada = new Chamada(privilegiado, List.of(comando));
        historico.add(chamada);
        String chave = chamada.chave();
        for (Map.Entry<String, LinuxCommandResult> e : respostas.entrySet()) {
            if (chave.startsWith(e.getKey()) || chave.contains(e.getKey())) {
                LinuxCommandResult base = e.getValue();
                return new LinuxCommandResult(base.exitCode(), base.stdout(), base.stderr(), privilegiado);
            }
        }
        if (fallback != null) {
            return fallback.apply(chamada, this);
        }
        return new LinuxCommandResult(0, "", "", privilegiado);
    }

    @Override
    public boolean comandoDisponivel(String comando) {
        return comandosDisponiveis.getOrDefault(comando, false);
    }
}

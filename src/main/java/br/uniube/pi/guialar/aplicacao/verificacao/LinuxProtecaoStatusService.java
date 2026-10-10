package br.uniube.pi.guialar.aplicacao.verificacao;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.dominio.diagnostico.StatusDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.io.IOException;
import java.util.List;
import java.util.Properties;

/**
 * Status da proteção Linux: manifesto + verificação real (smoke test).
 */
public class LinuxProtecaoStatusService {

    private final LinuxDnsManifestStore manifestStore;
    private final VerificacaoDnsService verificacaoDns;

    public LinuxProtecaoStatusService() {
        this(new LinuxDnsManifestStore(), new VerificacaoDnsService());
    }

    public LinuxProtecaoStatusService(LinuxDnsManifestStore manifestStore, VerificacaoDnsService verificacaoDns) {
        this.manifestStore = manifestStore;
        this.verificacaoDns = verificacaoDns;
    }

    public record Avaliacao(StatusDns status, String nota, String snippetNixos) {
    }

    public Avaliacao avaliarSomenteManifesto() {
        try {
            if (!manifestStore.existe()) {
                return new Avaliacao(StatusDns.NAO_APLICADO,
                    "Nenhuma alteração registrada pelo GuiaLar em ~/.guialar/.", null);
            }
            Properties props = manifestStore.carregar();
            String modo = props.getProperty(LinuxDnsManifestStore.KEY_MODO, "");
            if (LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS.equals(modo)) {
                String snippet = props.getProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, "")
                    .replace("\\n", "\n");
                return new Avaliacao(StatusDns.AGUARDANDO_APLICACAO,
                    "Copie o trecho no configuration.nix e rode nixos-rebuild switch; depois use Verificar de novo.",
                    snippet);
            }
            return new Avaliacao(StatusDns.DESCONHECIDO,
                "Há manifesto de DNS aplicado; use Verificar de novo para confirmar a proteção.", null);
        } catch (IOException e) {
            return new Avaliacao(StatusDns.DESCONHECIDO,
                "Não foi possível ler o manifesto: " + e.getMessage(), null);
        }
    }

    public Avaliacao avaliarComVerificacao() {
        Avaliacao base = avaliarSomenteManifesto();
        if (base.status() == StatusDns.AGUARDANDO_APLICACAO) {
            List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
            if (VerificacaoDnsService.protecaoConfirmada(resultados)) {
                try {
                    Properties props = manifestStore.carregar();
                    props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
                    manifestStore.salvar(props);
                } catch (IOException ignored) {
                }
                return new Avaliacao(StatusDns.APLICADO,
                    "Proteção confirmada: sites de teste bloqueados e example.com responde.", null);
            }
            return base;
        }
        if (!manifestStore.existe()) {
            List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
            if (VerificacaoDnsService.protecaoConfirmada(resultados)) {
                return new Avaliacao(StatusDns.PARCIAL,
                    "A verificação passou, mas não há manifesto GuiaLar (pode ter sido configurado manualmente).",
                    null);
            }
            return new Avaliacao(StatusDns.NAO_APLICADO,
                "Proteção da rede não confirmada pela verificação.", null);
        }
        List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
        if (VerificacaoDnsService.protecaoConfirmada(resultados)) {
            return new Avaliacao(StatusDns.APLICADO,
                "Proteção confirmada: sites de teste bloqueados e example.com responde.", null);
        }
        return new Avaliacao(StatusDns.NAO_APLICADO,
            "Há manifesto, mas a verificação não confirmou o filtro (malware.testcategory.com / example.com).",
            null);
    }

    public String carregarSnippetNixosPersistido() {
        try {
            if (!manifestStore.existe()) {
                return null;
            }
            Properties props = manifestStore.carregar();
            if (!LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS.equals(
                props.getProperty(LinuxDnsManifestStore.KEY_MODO))) {
                return null;
            }
            return props.getProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, "").replace("\\n", "\n");
        } catch (IOException e) {
            return null;
        }
    }
}

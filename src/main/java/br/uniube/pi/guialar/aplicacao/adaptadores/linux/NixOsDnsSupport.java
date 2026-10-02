package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;

/**
 * NixOS: não editar /etc gerenciado pelo Nix; oferecer snippet ou NM quando seguro.
 */
public class NixOsDnsSupport {

    private final LinuxCommandRunner runner;
    private final LinuxDnsBackendSelector selector;
    private final Path resolvConf;

    public NixOsDnsSupport(LinuxCommandRunner runner, LinuxDnsBackendSelector selector) {
        this(runner, selector, Path.of("/etc/resolv.conf"));
    }

    public NixOsDnsSupport(LinuxCommandRunner runner, LinuxDnsBackendSelector selector, Path resolvConf) {
        this.runner = runner;
        this.selector = selector;
        this.resolvConf = resolvConf;
    }

    /**
     * @return true se há DNS global que competiria com NM por conexão.
     */
    public boolean temDnsGlobalConflitante() {
        if (temNameserversEstaticosResolvConf()) {
            return true;
        }
        return temDnsGlobalResolved();
    }

    public boolean podeUsarNetworkManager() {
        return selector.networkManagerAtivo() && !temDnsGlobalConflitante();
    }

    public String gerarSnippetConfigurationNix(ServidorDns servidor) {
        return """
            # Cole no seu configuration.nix (remova outros networking.nameservers globais):
            networking.nameservers = [ "%s" "%s" "%s" "%s" ];
            # Depois: sudo nixos-rebuild switch
            """.formatted(
            servidor.getPrimario(),
            servidor.getSecundario(),
            servidor.getPrimarioIpv6(),
            servidor.getSecundarioIpv6()
        ).trim();
    }

    public String instrucaoDesfazerSnippet() {
        return """
            Para desfazer no NixOS:
            1. Remova o bloco networking.nameservers que você adicionou no configuration.nix
            2. Execute: sudo nixos-rebuild switch
            """.trim();
    }

    public String notaEnsureProfiles() {
        return "No NixOS, perfis do NetworkManager em /etc/NetworkManager/system-connections "
            + "costumam sobreviver ao nixos-rebuild, exceto se você usa "
            + "networking.networkmanager.ensureProfiles (que recria perfis).";
    }

    private boolean temNameserversEstaticosResolvConf() {
        try {
            if (!Files.exists(resolvConf)) {
                return false;
            }
            if (Files.isSymbolicLink(resolvConf)) {
                Path alvo = Files.readSymbolicLink(resolvConf);
                String s = alvo.toString().toLowerCase(Locale.ROOT);
                if (s.contains("systemd") || s.contains("stub") || s.contains("resolvconf")) {
                    return false;
                }
            }
            String conteudo = Files.readString(resolvConf);
            for (String linha : conteudo.split("\n")) {
                String t = linha.trim();
                if (t.startsWith("nameserver ") && !t.contains("127.0.0")) {
                    return true;
                }
            }
        } catch (Exception ignored) {
        }
        return false;
    }

    private boolean temDnsGlobalResolved() {
        try {
            if (!runner.comandoDisponivel("resolvectl")) {
                return false;
            }
            LinuxCommandResult r = runner.executar(false, "resolvectl", "status");
            if (!r.sucesso() || r.stdout() == null) {
                return false;
            }
            boolean emGlobal = false;
            for (String linha : r.stdout().split("\n")) {
                if (linha.startsWith("Global")) {
                    emGlobal = true;
                    continue;
                }
                if (emGlobal && linha.trim().startsWith("DNS Servers:")) {
                    String dns = linha.substring(linha.indexOf(':') + 1).trim();
                    return !dns.isEmpty() && !dns.equals("-");
                }
                if (emGlobal && linha.startsWith("Link ")) {
                    break;
                }
            }
        } catch (Exception ignored) {
        }
        return false;
    }
}

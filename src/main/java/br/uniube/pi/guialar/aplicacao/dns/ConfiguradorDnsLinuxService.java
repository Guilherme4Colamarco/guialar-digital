package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackend;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackendSelector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmConexaoEstado;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmManifestCodec;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmManifestValidator;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmcliTerseParser;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxPkexecMensagens;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxShellEscape;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxSystemPaths;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.NixOsDnsSupport;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.ProcessLinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Properties;

/**
 * Configuração/reversão de DNS no Linux: uma elevação pkexec por aplicar/desfazer.
 */
public class ConfiguradorDnsLinuxService {

    private static final String HEREDOC_TAG = "GUAILAR_EOF";

    private final LinuxCommandRunner runner;
    private final LinuxDnsBackendSelector selector;
    private final NixOsDnsSupport nixOs;
    private final LinuxDnsManifestStore manifestStore;
    private final VerificacaoDnsService verificacaoDns;

    public ConfiguradorDnsLinuxService() {
        this(new ProcessLinuxCommandRunner(), new LinuxDnsManifestStore());
    }

    public ConfiguradorDnsLinuxService(LinuxCommandRunner runner, LinuxDnsManifestStore manifestStore) {
        this.runner = runner;
        this.manifestStore = manifestStore;
        this.selector = new LinuxDnsBackendSelector(runner);
        this.nixOs = new NixOsDnsSupport(runner, selector);
        this.verificacaoDns = new VerificacaoDnsService();
    }

    public ConfiguradorDnsLinuxService(
            LinuxCommandRunner runner,
            LinuxDnsBackendSelector selector,
            NixOsDnsSupport nixOs,
            LinuxDnsManifestStore manifestStore) {
        this.runner = runner;
        this.selector = selector;
        this.nixOs = nixOs;
        this.manifestStore = manifestStore;
        this.verificacaoDns = new VerificacaoDnsService();
    }

    public LinuxDnsBackendSelector getSelector() {
        return selector;
    }

    public LinuxDnsManifestStore getManifestStore() {
        return manifestStore;
    }

    public ConfiguracaoDns configurar(InfoDistro distro) {
        ServidorDns servidor = ServidorDns.getPadrao();
        if (distro.getTipo() == TipoDistro.DESCONHECIDA && !selector.isNixOs()) {
            return ConfiguracaoDns.erro("unknown", "Distribuição não suportada");
        }

        if (distro.getTipo() == TipoDistro.NIXOS || selector.isNixOs()) {
            return configurarNixOs(servidor);
        }

        LinuxDnsBackend backend = selector.selecionar(distro);
        ConfiguracaoDns aplicado = switch (backend) {
            case NETWORK_MANAGER -> configurarNetworkManager(servidor);
            case NETPLAN -> configurarNetplan(servidor);
            case SYSTEMD_RESOLVED -> configurarSystemdResolved(servidor);
            case RESOLV_CONF -> configurarResolvConf(servidor);
            case NIXOS_SNIPPET -> ConfiguracaoDns.erro("NixOS", "ramo inesperado");
        };
        return finalizarComVerificacao(aplicado, servidor);
    }

    private ConfiguracaoDns finalizarComVerificacao(ConfiguracaoDns aplicado, ServidorDns servidor) {
        if (aplicado.isAguardandoUsuario() || !aplicado.isAplicado()) {
            return aplicado;
        }
        List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
        if (VerificacaoDnsService.protecaoConfirmada(resultados)) {
            return aplicado;
        }
        return ConfiguracaoDns.sucessoComAviso(servidor, aplicado.getMetodoConfiguracao(),
            "DNS gravado, mas a verificação ainda não confirmou o filtro. Use Verificar de novo.");
    }

    private ConfiguracaoDns configurarNixOs(ServidorDns servidor) {
        if (nixOs.podeUsarNetworkManager()) {
            ConfiguracaoDns r = finalizarComVerificacao(configurarNetworkManager(servidor), servidor);
            if (r.isAplicado()) {
                return ConfiguracaoDns.sucessoComAviso(servidor, r.getMetodoConfiguracao(),
                    nixOs.notaEnsureProfiles());
            }
            return r;
        }
        String snippet = nixOs.gerarSnippetConfigurationNix(servidor);
        try {
            Properties props = new Properties();
            props.setProperty(LinuxDnsManifestStore.KEY_METODO, LinuxDnsBackend.NIXOS_SNIPPET.getRotulo());
            props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS);
            props.setProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, snippet.replace("\n", "\\n"));
            manifestStore.salvar(props);
        } catch (IOException e) {
            return ConfiguracaoDns.erro("NixOS", "Não foi possível salvar manifesto: " + e.getMessage());
        }
        return ConfiguracaoDns.aguardandoAplicacaoUsuario(
            LinuxDnsBackend.NIXOS_SNIPPET.getRotulo(),
            snippet,
            "No NixOS, copie o trecho para o configuration.nix e rode sudo nixos-rebuild switch. "
                + "O status ficará em \"aguardando você aplicar\" até a verificação passar."
        );
    }

    private ConfiguracaoDns configurarNetworkManager(ServidorDns servidor) {
        try {
            LinuxCommandResult ativas = runner.executar(false, "nmcli", "-t", "-f", "NAME,TYPE,DEVICE",
                "connection", "show", "--active");
            List<LinuxNmcliTerseParser.ConexaoAtiva> todas =
                LinuxNmcliTerseParser.parseConexoesAtivas(ativas.stdout());

            List<String> vpns = new ArrayList<>();
            List<String> alvos = new ArrayList<>();
            for (LinuxNmcliTerseParser.ConexaoAtiva c : todas) {
                if (c.isVpnOuTunel()) {
                    vpns.add(c.nome());
                    continue;
                }
                if (c.isRedeFisicaOuWifi()) {
                    alvos.add(c.nome());
                }
            }
            if (alvos.isEmpty()) {
                return ConfiguracaoDns.erro("NetworkManager",
                    "Nenhuma conexão ethernet/Wi‑Fi ativa encontrada (VPN não é alterada).");
            }

            List<LinuxNmConexaoEstado> backups = new ArrayList<>();
            LinuxNmManifestValidator validator = new LinuxNmManifestValidator(runner);
            for (String nome : alvos) {
                LinuxNmConexaoEstado estado = lerEstadoConexao(nome);
                LinuxNmManifestValidator.Resultado vr = validator.validarEstadoAoVivo(estado);
                if (!vr.valido()) {
                    return ConfiguracaoDns.erro("NetworkManager", vr.mensagem());
                }
                backups.add(vr.conexoes().get(0));
            }

            String script = montarScriptNmAplicar(servidor, backups);
            LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
            ConfiguracaoDns falha = interpretarPkexec("NetworkManager", elevado, false);
            if (falha != null) {
                return falha;
            }

            Properties props = new Properties();
            props.setProperty(LinuxDnsManifestStore.KEY_METODO, LinuxDnsBackend.NETWORK_MANAGER.getRotulo());
            props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
            LinuxNmManifestCodec.gravarConexoes(props, backups);
            manifestStore.salvar(props);

            if (!vpns.isEmpty()) {
                return ConfiguracaoDns.sucessoComAviso(servidor, "NetworkManager",
                    "VPN detectada (" + String.join(", ", vpns) + ") — não alteramos a VPN; só ethernet/Wi‑Fi.");
            }
            return ConfiguracaoDns.sucesso(servidor, "NetworkManager");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("NetworkManager", e.getMessage());
        }
    }

    private LinuxNmConexaoEstado lerEstadoConexao(String nome) throws IOException, InterruptedException {
        LinuxCommandResult uuidResult = runner.executar(false, "nmcli", "-t", "-g", "connection.uuid",
            "connection", "show", nome);
        String uuid = uuidResult.stdout() != null ? uuidResult.stdout().trim() : "";
        LinuxCommandResult r = runner.executar(false, "nmcli", "-t", "-f",
            "ipv4.dns,ipv6.dns,ipv4.ignore-auto-dns,ipv6.ignore-auto-dns",
            "connection", "show", nome);
        Map<String, String> campos = LinuxNmcliTerseParser.parseCamposDns(r.stdout());
        return new LinuxNmConexaoEstado(
            uuid,
            nome,
            campos.getOrDefault("ipv4.dns", ""),
            campos.getOrDefault("ipv6.dns", ""),
            campos.getOrDefault("ipv4.ignore-auto-dns", "no"),
            campos.getOrDefault("ipv6.ignore-auto-dns", "no")
        );
    }

    private String montarScriptNmAplicar(ServidorDns servidor, List<LinuxNmConexaoEstado> backups) {
        String v4 = servidor.getPrimario() + " " + servidor.getSecundario();
        String v6 = servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6();
        StringBuilder sb = new StringBuilder();
        sb.append("set -e\n");
        sb.append("guialar_nm_rollback() {\n");
        for (LinuxNmConexaoEstado b : backups) {
            sb.append(montarBlocoNmRestaurar(b));
        }
        sb.append("}\n");
        sb.append("trap guialar_nm_rollback ERR\n");
        for (LinuxNmConexaoEstado b : backups) {
            String q = LinuxShellEscape.shSingleQuote(b.uuid());
            sb.append("nmcli connection modify ").append(q).append(" ipv4.dns ")
                .append(LinuxShellEscape.shSingleQuote(v4)).append("\n");
            sb.append("nmcli connection modify ").append(q).append(" ipv4.ignore-auto-dns ")
                .append(LinuxShellEscape.shSingleQuote("yes")).append("\n");
            sb.append("nmcli connection modify ").append(q).append(" ipv6.dns ")
                .append(LinuxShellEscape.shSingleQuote(v6)).append("\n");
            sb.append("nmcli connection modify ").append(q).append(" ipv6.ignore-auto-dns ")
                .append(LinuxShellEscape.shSingleQuote("yes")).append("\n");
            sb.append("nmcli connection up ").append(q).append("\n");
        }
        sb.append("trap - ERR\n");
        return sb.toString();
    }

    private String montarBlocoNmRestaurar(LinuxNmConexaoEstado c) {
        String q = LinuxShellEscape.shSingleQuote(c.uuid());
        StringBuilder sb = new StringBuilder();
        sb.append("nmcli connection modify ").append(q).append(" ipv4.dns ")
            .append(LinuxShellEscape.shSingleQuote(c.ipv4Dns())).append("\n");
        sb.append("nmcli connection modify ").append(q).append(" ipv4.ignore-auto-dns ")
            .append(LinuxShellEscape.shSingleQuote(c.ipv4IgnoreAutoDns())).append("\n");
        sb.append("nmcli connection modify ").append(q).append(" ipv6.dns ")
            .append(LinuxShellEscape.shSingleQuote(c.ipv6Dns())).append("\n");
        sb.append("nmcli connection modify ").append(q).append(" ipv6.ignore-auto-dns ")
            .append(LinuxShellEscape.shSingleQuote(c.ipv6IgnoreAutoDns())).append("\n");
        sb.append("nmcli connection up ").append(q).append("\n");
        return sb.toString();
    }

    private ConfiguracaoDns configurarNetplan(ServidorDns servidor) {
        try {
            String conteudo = String.join("\n", montarYamlNetplan(servidor)) + "\n";
            String script = "set -e\nmkdir -p /etc/netplan\n"
                + "cat > " + LinuxSystemPaths.NETPLAN_GUAILAR + " << '" + HEREDOC_TAG + "'\n"
                + conteudo
                + HEREDOC_TAG + "\n"
                + "netplan apply\n";
            LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
            ConfiguracaoDns falha = interpretarPkexec("Netplan", elevado, false);
            if (falha != null) {
                return falha;
            }
            salvarManifestoSimples(LinuxDnsBackend.NETPLAN.getRotulo());
            return ConfiguracaoDns.sucesso(servidor, "Netplan");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("Netplan", e.getMessage());
        }
    }

    private ConfiguracaoDns configurarSystemdResolved(ServidorDns servidor) {
        try {
            String conteudo = String.join("\n", montarResolvedDropin(servidor)) + "\n";
            String script = "set -e\nmkdir -p " + LinuxSystemPaths.DIR_RESOLVED_DROPIN + "\n"
                + "cat > " + LinuxSystemPaths.DROPIN_RESOLVED + " << '" + HEREDOC_TAG + "'\n"
                + conteudo
                + HEREDOC_TAG + "\n"
                + "systemctl restart systemd-resolved\n";
            LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
            ConfiguracaoDns falha = interpretarPkexec("systemd-resolved", elevado, false);
            if (falha != null) {
                return falha;
            }
            salvarManifestoSimples(LinuxDnsBackend.SYSTEMD_RESOLVED.getRotulo());
            return ConfiguracaoDns.sucesso(servidor, "systemd-resolved (drop-in 99-guialar.conf)");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("systemd-resolved", e.getMessage());
        }
    }

    private ConfiguracaoDns configurarResolvConf(ServidorDns servidor) {
        try {
            Path resolvConf = Path.of(LinuxSystemPaths.RESOLV_CONF);
            if (Files.isSymbolicLink(resolvConf)) {
                Path target = Files.readSymbolicLink(resolvConf);
                if (target.toString().contains("systemd") || target.toString().contains("stub")) {
                    return ConfiguracaoDns.erro("resolv.conf",
                        "É um stub do systemd. Use systemd-resolved ou NetworkManager.");
                }
            }
            String conteudo = montarResolvConf(servidor);
            String script = "set -e\n"
                + "if [ -f " + LinuxSystemPaths.RESOLV_CONF + " ]; then cp "
                + LinuxSystemPaths.RESOLV_CONF + " " + LinuxSystemPaths.RESOLV_GUAILAR_BACKUP + "; fi\n"
                + "cat > " + LinuxSystemPaths.RESOLV_CONF + " << '" + HEREDOC_TAG + "'\n"
                + conteudo
                + HEREDOC_TAG + "\n";
            LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
            ConfiguracaoDns falha = interpretarPkexec("resolv.conf", elevado, false);
            if (falha != null) {
                return falha;
            }
            salvarManifestoSimples(LinuxDnsBackend.RESOLV_CONF.getRotulo());
            return ConfiguracaoDns.sucesso(servidor, "resolv.conf");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("resolv.conf", e.getMessage());
        }
    }

    public ConfiguracaoDns desfazer() {
        if (!manifestStore.existe()) {
            return ConfiguracaoDns.naoAplicado("Linux", "Nenhum manifesto em ~/.guialar/");
        }
        try {
            Properties props = manifestStore.carregar();
            String modo = props.getProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
            String metodo = props.getProperty(LinuxDnsManifestStore.KEY_METODO, "");

            if (LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS.equals(modo)) {
                String snippet = props.getProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, "")
                    .replace("\\n", "\n");
                manifestStore.remover();
                return new ConfiguracaoDns(null, false, metodo,
                    "Remova o trecho abaixo do configuration.nix e execute sudo nixos-rebuild switch:\n\n"
                        + snippet);
            }

            ConfiguracaoDns resultado;
            if (metodo.contains("NetworkManager")) {
                resultado = reverterNetworkManager(props);
            } else if (metodo.contains("Netplan")) {
                resultado = reverterNetplan();
            } else if (metodo.contains("systemd-resolved")) {
                resultado = reverterSystemdResolved();
            } else if (metodo.contains("resolv.conf")) {
                resultado = reverterResolvConf();
            } else {
                return ConfiguracaoDns.erro(metodo, "Método desconhecido no manifesto");
            }
            if (resultado.isAplicado()) {
                manifestStore.remover();
            }
            return resultado;
        } catch (Exception e) {
            return ConfiguracaoDns.erro("Linux", e.getMessage());
        }
    }

    public String getInstrucoesReversao(InfoDistro distro) {
        if (selector.isNixOs() && !nixOs.podeUsarNetworkManager()) {
            return nixOs.instrucaoDesfazerSnippet();
        }
        return """
            Para reverter o DNS no Linux use Desfazer proteção no GuiaLar (manifesto em ~/.guialar/).

            systemd-resolved: remove /etc/systemd/resolved.conf.d/99-guialar.conf e reinicia o serviço.
            """.trim();
    }

    private ConfiguracaoDns reverterNetworkManager(Properties props) throws Exception {
        List<LinuxNmConexaoEstado> conexoes = LinuxNmManifestCodec.lerConexoes(props);
        LinuxNmManifestValidator validator = new LinuxNmManifestValidator(runner);
        LinuxNmManifestValidator.Resultado validado = validator.validarParaRevert(conexoes);
        if (!validado.valido()) {
            return ConfiguracaoDns.erro("NetworkManager", validado.mensagem());
        }
        StringBuilder sb = new StringBuilder();
        sb.append("set -e\n");
        for (LinuxNmConexaoEstado c : validado.conexoes()) {
            sb.append(montarBlocoNmRestaurar(c));
        }
        LinuxCommandResult elevado = runner.executarScriptPrivilegiado(sb.toString());
        ConfiguracaoDns falha = interpretarPkexec("NetworkManager", elevado, true);
        if (falha != null) {
            return falha;
        }
        return new ConfiguracaoDns(null, true, "NetworkManager",
            "DNS das conexões registradas foi restaurado.");
    }

    private ConfiguracaoDns reverterNetplan() throws Exception {
        String script = """
            set -e
            rm -f %s
            netplan apply
            """.formatted(LinuxSystemPaths.NETPLAN_GUAILAR);
        LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
        ConfiguracaoDns falha = interpretarPkexec("Netplan", elevado, true);
        if (falha != null) {
            return falha;
        }
        return new ConfiguracaoDns(null, true, "Netplan", "Arquivo Netplan do GuiaLar removido.");
    }

    private ConfiguracaoDns reverterSystemdResolved() throws Exception {
        String script = """
            set -e
            rm -f %s
            systemctl restart systemd-resolved
            """.formatted(LinuxSystemPaths.DROPIN_RESOLVED);
        LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
        ConfiguracaoDns falha = interpretarPkexec("systemd-resolved", elevado, true);
        if (falha != null) {
            return falha;
        }
        return new ConfiguracaoDns(null, true, "systemd-resolved",
            "Arquivo /etc/systemd/resolved.conf.d/99-guialar.conf removido e systemd-resolved reiniciado.");
    }

    private ConfiguracaoDns reverterResolvConf() throws Exception {
        String script = """
            set -e
            if [ -f %s ]; then cp %s %s; else exit 1; fi
            """.formatted(
            LinuxSystemPaths.RESOLV_GUAILAR_BACKUP,
            LinuxSystemPaths.RESOLV_GUAILAR_BACKUP,
            LinuxSystemPaths.RESOLV_CONF
        );
        LinuxCommandResult elevado = runner.executarScriptPrivilegiado(script);
        ConfiguracaoDns falha = interpretarPkexec("resolv.conf", elevado, true);
        if (falha != null) {
            return falha;
        }
        return new ConfiguracaoDns(null, true, "resolv.conf", "Backup /etc/resolv.conf.guialar-backup restaurado.");
    }

    private void salvarManifestoSimples(String metodo) throws IOException {
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, metodo);
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        manifestStore.salvar(props);
    }

    private ConfiguracaoDns interpretarPkexec(String metodo, LinuxCommandResult r, boolean revertendo) {
        if (r.sucesso()) {
            return null;
        }
        String msgUsuario = LinuxPkexecMensagens.mensagemUsuario(r.exitCode());
        if (msgUsuario != null) {
            return ConfiguracaoDns.naoAplicado(metodo, msgUsuario);
        }
        String detalhe = r.saidaCombinada();
        if (revertendo) {
            return ConfiguracaoDns.erro(metodo,
                "Não foi possível desfazer completamente. O manifesto foi mantido. " + detalhe);
        }
        return ConfiguracaoDns.erro(metodo,
            "A operação elevada falhou antes de concluir; o manifesto não foi salvo. "
                + "Se algo mudou na rede, anote o horário e peça ajuda. " + detalhe);
    }

    private List<String> montarYamlNetplan(ServidorDns servidor) {
        List<String> linhas = new ArrayList<>();
        linhas.add("# Configurado por GuiaLar Digital");
        linhas.add("network:");
        linhas.add("  version: 2");
        linhas.add("  ethernets:");
        linhas.add("    all:");
        linhas.add("      match:");
        linhas.add("        name: en*");
        linhas.add("      dhcp4: true");
        linhas.add("      dhcp6: true");
        linhas.add("      nameservers:");
        linhas.add("        addresses:");
        linhas.add("          - " + servidor.getPrimario());
        linhas.add("          - " + servidor.getSecundario());
        linhas.add("          - " + servidor.getPrimarioIpv6());
        linhas.add("          - " + servidor.getSecundarioIpv6());
        return linhas;
    }

    private List<String> montarResolvedDropin(ServidorDns servidor) {
        List<String> linhas = new ArrayList<>();
        linhas.add("# Configurado por GuiaLar Digital");
        linhas.add("[Resolve]");
        linhas.add("DNS=" + servidor.getPrimario() + " " + servidor.getSecundario() + " "
            + servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6());
        linhas.add("FallbackDNS=");
        linhas.add("Domains=~.");
        linhas.add("DNSSEC=allow-downgrade");
        linhas.add("DNSOverTLS=opportunistic");
        return linhas;
    }

    private String montarResolvConf(ServidorDns servidor) {
        return """
            # Configurado por GuiaLar Digital
            nameserver %s
            nameserver %s
            nameserver %s
            nameserver %s
            """.formatted(
            servidor.getPrimario(),
            servidor.getSecundario(),
            servidor.getPrimarioIpv6(),
            servidor.getSecundarioIpv6()
        ).trim() + "\n";
    }
}

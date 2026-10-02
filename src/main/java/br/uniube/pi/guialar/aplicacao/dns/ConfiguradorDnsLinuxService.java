package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackend;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackendSelector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.NixOsDnsSupport;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.ProcessLinuxCommandRunner;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;
import java.util.Properties;

/**
 * Configuração/reversão de DNS no Linux: GUI como usuário, escrita elevada via pkexec.
 */
public class ConfiguradorDnsLinuxService {

    private static final String BACKUP_SUFFIX = ".guialar-backup-";
    private static final String DROPIN_RESOLVED = "/etc/systemd/resolved.conf.d/99-guialar.conf";

    private final LinuxCommandRunner runner;
    private final LinuxDnsBackendSelector selector;
    private final NixOsDnsSupport nixOs;
    private final LinuxDnsManifestStore manifestStore;

    public ConfiguradorDnsLinuxService() {
        this(new ProcessLinuxCommandRunner(), new LinuxDnsManifestStore());
    }

    public ConfiguradorDnsLinuxService(LinuxCommandRunner runner, LinuxDnsManifestStore manifestStore) {
        this.runner = runner;
        this.manifestStore = manifestStore;
        this.selector = new LinuxDnsBackendSelector(runner);
        this.nixOs = new NixOsDnsSupport(runner, selector);
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
        return switch (backend) {
            case NETWORK_MANAGER -> configurarNetworkManager(servidor);
            case NETPLAN -> configurarNetplan(servidor);
            case SYSTEMD_RESOLVED -> configurarSystemdResolved(servidor);
            case RESOLV_CONF -> configurarResolvConf(servidor);
            case NIXOS_SNIPPET -> ConfiguracaoDns.erro("NixOS", "ramo inesperado");
        };
    }

    private ConfiguracaoDns configurarNixOs(ServidorDns servidor) {
        if (nixOs.podeUsarNetworkManager()) {
            ConfiguracaoDns r = configurarNetworkManager(servidor);
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
            String conexao = obterConexaoAtiva();
            if (conexao == null) {
                return ConfiguracaoDns.erro("NetworkManager", "Nenhuma conexão ativa encontrada");
            }

            Path backupFile = backupConfigNetworkManager(conexao);

            LinuxCommandResult r1 = runner.executar(true, "nmcli", "connection", "modify", conexao,
                "ipv4.dns", servidor.getPrimario() + " " + servidor.getSecundario());
            if (!r1.sucesso()) {
                return falhaPkexec("NetworkManager", r1);
            }
            runner.executar(true, "nmcli", "connection", "modify", conexao,
                "ipv4.ignore-auto-dns", "yes");
            runner.executar(true, "nmcli", "connection", "modify", conexao,
                "ipv6.dns", servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6());
            runner.executar(true, "nmcli", "connection", "modify", conexao,
                "ipv6.ignore-auto-dns", "yes");
            LinuxCommandResult up = runner.executar(true, "nmcli", "connection", "up", conexao);
            if (!up.sucesso()) {
                return falhaPkexec("NetworkManager", up);
            }

            salvarManifestoAplicado(LinuxDnsBackend.NETWORK_MANAGER.getRotulo(), conexao, backupFile, null);
            return ConfiguracaoDns.sucesso(servidor, "NetworkManager");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("NetworkManager", e.getMessage());
        }
    }

    private ConfiguracaoDns configurarNetplan(ServidorDns servidor) {
        try {
            Path netplanDir = Path.of("/etc/netplan");
            Path configFile = netplanDir.resolve("99-guialar-dns.yaml");

            List<String> linhas = montarYamlNetplan(servidor, configFile);

            Path staging = stagingUsuario("99-guialar-dns.yaml");
            Files.write(staging, linhas, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING);

            runner.executar(true, "mkdir", "-p", netplanDir.toString());
            LinuxCommandResult cp = runner.executar(true, "cp", staging.toString(), configFile.toString());
            if (!cp.sucesso()) {
                return falhaPkexec("Netplan", cp);
            }
            LinuxCommandResult apply = runner.executar(true, "netplan", "apply");
            if (!apply.sucesso()) {
                return falhaPkexec("Netplan", apply);
            }

            salvarManifestoAplicado(LinuxDnsBackend.NETPLAN.getRotulo(), null, null, configFile);
            return ConfiguracaoDns.sucesso(servidor, "Netplan");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("Netplan", e.getMessage());
        }
    }

    private ConfiguracaoDns configurarSystemdResolved(ServidorDns servidor) {
        try {
            List<String> linhas = new ArrayList<>();
            linhas.add("# Configurado por GuiaLar Digital");
            linhas.add("# Para reverter: remova /etc/systemd/resolved.conf.d/99-guialar.conf e reinicie systemd-resolved");
            linhas.add("");
            linhas.add("[Resolve]");
            linhas.add("DNS=" + servidor.getPrimario() + " " + servidor.getSecundario() + " "
                + servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6());
            linhas.add("FallbackDNS=");
            linhas.add("Domains=~.");
            linhas.add("DNSSEC=allow-downgrade");
            linhas.add("DNSOverTLS=opportunistic");

            Path staging = stagingUsuario("99-guialar.conf");
            Files.write(staging, linhas, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING);

            runner.executar(true, "mkdir", "-p", "/etc/systemd/resolved.conf.d");
            LinuxCommandResult cp = runner.executar(true, "cp", staging.toString(), DROPIN_RESOLVED);
            if (!cp.sucesso()) {
                return falhaPkexec("systemd-resolved", cp);
            }
            LinuxCommandResult restart = runner.executar(true, "systemctl", "restart", "systemd-resolved");
            if (!restart.sucesso()) {
                return falhaPkexec("systemd-resolved", restart);
            }

            salvarManifestoAplicado(LinuxDnsBackend.SYSTEMD_RESOLVED.getRotulo(), null, null, Path.of(DROPIN_RESOLVED));
            return ConfiguracaoDns.sucesso(servidor, "systemd-resolved (drop-in 99-guialar.conf)");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("systemd-resolved", e.getMessage());
        }
    }

    private ConfiguracaoDns configurarResolvConf(ServidorDns servidor) {
        try {
            Path resolvConf = Path.of("/etc/resolv.conf");
            if (Files.isSymbolicLink(resolvConf)) {
                Path target = Files.readSymbolicLink(resolvConf);
                if (target.toString().contains("systemd") || target.toString().contains("stub")) {
                    return ConfiguracaoDns.erro("resolv.conf",
                        "É um stub do systemd. Use systemd-resolved ou NetworkManager.");
                }
            }

            List<String> linhas = new ArrayList<>();
            linhas.add("# Configurado por GuiaLar Digital");
            linhas.add("nameserver " + servidor.getPrimario());
            linhas.add("nameserver " + servidor.getSecundario());
            linhas.add("nameserver " + servidor.getPrimarioIpv6());
            linhas.add("nameserver " + servidor.getSecundarioIpv6());

            String timestamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"));
            if (Files.exists(resolvConf)) {
                Path backupRemoto = Path.of("/etc/resolv.conf" + BACKUP_SUFFIX + timestamp);
                LinuxCommandResult cpBak = runner.executar(true, "cp", resolvConf.toString(), backupRemoto.toString());
                if (!cpBak.sucesso()) {
                    return falhaPkexec("resolv.conf", cpBak);
                }
            }

            Path staging = stagingUsuario("resolv.conf");
            Files.write(staging, linhas, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING);
            LinuxCommandResult cp = runner.executar(true, "cp", staging.toString(), resolvConf.toString());
            if (!cp.sucesso()) {
                return falhaPkexec("resolv.conf", cp);
            }

            salvarManifestoAplicado(LinuxDnsBackend.RESOLV_CONF.getRotulo(), null, null, resolvConf);
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

            if (LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS.equals(modo)
                    || metodo.contains("NixOS")) {
                String snippet = props.getProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, "")
                    .replace("\\n", "\n");
                manifestStore.remover();
                return new ConfiguracaoDns(null, false, metodo,
                    "Remova o trecho abaixo do configuration.nix e execute sudo nixos-rebuild switch:\n\n"
                        + snippet);
            }

            if (metodo.contains("NetworkManager")) {
                return reverterNetworkManager(props);
            }
            if (metodo.contains("Netplan")) {
                return reverterNetplan(props);
            }
            if (metodo.contains("systemd-resolved")) {
                return reverterSystemdResolved();
            }
            if (metodo.contains("resolv.conf")) {
                return reverterResolvConf();
            }
            return ConfiguracaoDns.erro(metodo, "Método desconhecido no manifesto");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("Linux", e.getMessage());
        }
    }

    public String getInstrucoesReversao(InfoDistro distro) {
        if (selector.isNixOs() && !nixOs.podeUsarNetworkManager()) {
            return nixOs.instrucaoDesfazerSnippet();
        }
        StringBuilder sb = new StringBuilder("Para reverter o DNS no Linux:\n\n");
        sb.append("systemd-resolved:\n");
        sb.append("  sudo rm /etc/systemd/resolved.conf.d/99-guialar.conf\n");
        sb.append("  sudo systemctl restart systemd-resolved\n\n");
        sb.append("NetworkManager: use \"Desfazer\" no GuiaLar (manifesto em ~/.guialar/).\n");
        return sb.toString();
    }

    private ConfiguracaoDns reverterNetworkManager(Properties props) throws Exception {
        String conexao = props.getProperty(LinuxDnsManifestStore.KEY_CONEXAO);
        if (conexao == null || conexao.isBlank()) {
            throw new Exception("Conexão NM não registrada no manifesto");
        }
        LinuxCommandResult r1 = runner.executar(true, "nmcli", "connection", "modify", conexao, "ipv4.dns", "");
        if (!r1.sucesso()) {
            return falhaPkexec("NetworkManager", r1);
        }
        runner.executar(true, "nmcli", "connection", "modify", conexao, "ipv4.ignore-auto-dns", "no");
        runner.executar(true, "nmcli", "connection", "modify", conexao, "ipv6.dns", "");
        runner.executar(true, "nmcli", "connection", "modify", conexao, "ipv6.ignore-auto-dns", "no");
        runner.executar(true, "nmcli", "connection", "up", conexao);
        manifestStore.remover();
        return new ConfiguracaoDns(null, true, "NetworkManager", "DNS da conexão \"" + conexao + "\" revertido");
    }

    private ConfiguracaoDns reverterNetplan(Properties props) throws Exception {
        String arquivo = props.getProperty(LinuxDnsManifestStore.KEY_ARQUIVO_NETPLAN,
            "/etc/netplan/99-guialar-dns.yaml");
        LinuxCommandResult rm = runner.executar(true, "rm", "-f", arquivo);
        if (!rm.sucesso()) {
            return falhaPkexec("Netplan", rm);
        }
        runner.executar(true, "netplan", "apply");
        manifestStore.remover();
        return new ConfiguracaoDns(null, true, "Netplan", "Arquivo Netplan do GuiaLar removido");
    }

    private ConfiguracaoDns reverterSystemdResolved() throws Exception {
        LinuxCommandResult rm = runner.executar(true, "rm", "-f", DROPIN_RESOLVED);
        if (!rm.sucesso()) {
            return falhaPkexec("systemd-resolved", rm);
        }
        LinuxCommandResult restart = runner.executar(true, "systemctl", "restart", "systemd-resolved");
        if (!restart.sucesso()) {
            return falhaPkexec("systemd-resolved", restart);
        }
        manifestStore.remover();
        return new ConfiguracaoDns(null, true, "systemd-resolved",
            "Arquivo /etc/systemd/resolved.conf.d/99-guialar.conf removido e systemd-resolved reiniciado");
    }

    private ConfiguracaoDns reverterResolvConf() throws Exception {
        Path etcDir = Path.of("/etc");
        Path backup = null;
        try (var stream = Files.newDirectoryStream(etcDir, "resolv.conf.guialar-backup-*")) {
            List<Path> lista = new ArrayList<>();
            for (Path p : stream) {
                lista.add(p);
            }
            lista.sort(java.util.Comparator.comparing(Path::toString).reversed());
            if (!lista.isEmpty()) {
                backup = lista.get(0);
            }
        }
        if (backup == null) {
            throw new Exception("Backup de resolv.conf não encontrado em /etc");
        }
        LinuxCommandResult cp = runner.executar(true, "cp", backup.toString(), "/etc/resolv.conf");
        if (!cp.sucesso()) {
            return falhaPkexec("resolv.conf", cp);
        }
        manifestStore.remover();
        return new ConfiguracaoDns(null, true, "resolv.conf", "Backup restaurado: " + backup.getFileName());
    }

    private void salvarManifestoAplicado(String metodo, String conexao, Path backupNm, Path arquivo)
            throws IOException {
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, metodo);
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        if (conexao != null) {
            props.setProperty(LinuxDnsManifestStore.KEY_CONEXAO, conexao);
        }
        if (backupNm != null) {
            props.setProperty(LinuxDnsManifestStore.KEY_BACKUP_NM, backupNm.toString());
        }
        if (arquivo != null && metodo.contains("Netplan")) {
            props.setProperty(LinuxDnsManifestStore.KEY_ARQUIVO_NETPLAN, arquivo.toString());
        }
        manifestStore.salvar(props);
    }

    private Path stagingUsuario(String nome) throws IOException {
        Path dir = Path.of(System.getProperty("user.home"), ".guialar", "staging");
        Files.createDirectories(dir);
        return dir.resolve(nome);
    }

    private Path backupConfigNetworkManager(String conexao) throws IOException, InterruptedException {
        LinuxCommandResult processDns = runner.executar(false, "nmcli", "-t", "-f", "ipv4.dns,ipv6.dns",
            "connection", "show", conexao);
        List<String> config = new ArrayList<>();
        if (processDns.stdout() != null) {
            for (String linha : processDns.stdout().split("\n")) {
                config.add(linha);
            }
        }
        String timestamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"));
        Path backupDir = Path.of(System.getProperty("user.home"), ".guialar", "backups");
        Files.createDirectories(backupDir);
        Path backupFile = backupDir.resolve("nm-" + conexao + "-" + timestamp + ".txt");
        Files.write(backupFile, config);
        return backupFile;
    }

    private String obterConexaoAtiva() throws IOException, InterruptedException {
        LinuxCommandResult process = runner.executar(false, "nmcli", "-t", "-f", "NAME,TYPE,DEVICE",
            "connection", "show", "--active");

        String melhorConexao = null;
        String[] tiposIgnorar = {"vpn", "tun", "docker", "bridge", "veth"};
        String[] tiposPreferidos = {"802-3-ethernet", "ethernet", "802-11-wireless", "wifi"};

        if (process.stdout() == null) {
            return null;
        }
        for (String linha : process.stdout().split("\n")) {
            String[] partes = linha.split(":");
            if (partes.length < 3) {
                continue;
            }
            String nome = partes[0];
            String tipo = partes[1].toLowerCase();
            String device = partes[2];

            boolean ignorar = false;
            for (String tipoIgnorar : tiposIgnorar) {
                if (tipo.contains(tipoIgnorar) || device.contains(tipoIgnorar)) {
                    ignorar = true;
                    break;
                }
            }
            if (ignorar) {
                continue;
            }
            for (String tipoPreferido : tiposPreferidos) {
                if (tipo.contains(tipoPreferido)) {
                    melhorConexao = nome;
                    break;
                }
            }
            if (melhorConexao == null) {
                melhorConexao = nome;
            }
            if (tipo.contains("ethernet") || tipo.contains("802-3")) {
                break;
            }
        }
        return melhorConexao;
    }

    private List<String> montarYamlNetplan(ServidorDns servidor, Path configFile) {
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

    private ConfiguracaoDns falhaPkexec(String metodo, LinuxCommandResult r) {
        if (r.exitCode() == 127 && r.stderr() != null && r.stderr().contains("pkexec")) {
            return ConfiguracaoDns.naoAplicado(metodo, r.stderr());
        }
        if (r.exitCode() == 126) {
            return ConfiguracaoDns.naoAplicado(metodo,
                "Autorização cancelada ou negada no pkexec (polkit). O sistema não foi alterado.");
        }
        return ConfiguracaoDns.erro(metodo, r.saidaCombinada());
    }
}

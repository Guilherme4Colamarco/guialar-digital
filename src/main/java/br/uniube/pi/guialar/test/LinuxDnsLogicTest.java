package br.uniube.pi.guialar.test;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.FakeLinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackend;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackendSelector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsFamiliesDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNetplanPrivilegedScript;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNetplanSupport;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmConexaoEstado;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmManifestCodec;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmManifestValidator;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxNmcliTerseParser;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxOsRelease;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxPkexecMensagens;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxSystemPaths;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.NixOsDnsSupport;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsLinuxService;
import br.uniube.pi.guialar.aplicacao.verificacao.LinuxProtecaoStatusService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.diagnostico.StatusDns;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Properties;

public class LinuxDnsLogicTest {

    private static int falhas = 0;

    public static void main(String[] args) throws Exception {
        System.out.println("=== GuiaLar Linux DNS logic tests ===");
        testOsReleaseNixOs();
        testNmPreferidoSobreNetplan();
        testParserEscapadoDoisPontos();
        testFamiliesNaoConfunde11130();
        testUmPkexecPorAplicarNm();
        testScriptContemIpv6EIgnoreAutoDns();
        testResolvedRestartNoScript();
        testPkexec126e127();
        testRevertResolvedUmScript();
        testMultiplasConexoesNoScript();
        testNixOsComNmSemDnsGlobal();
        testNixOsNmComDnsGlobalVaiParaSnippet();
        testStatusAguardandoPersisteNoManifesto();
        testRevertNaoApagaManifestoSeFalhar();
        testExtrairInterfacesNetplan();
        testGerarYamlNetplan();
        testSepararComDnsProprioNetplan();
        testNetplanScriptBackupApplyRevert();
        testNetplanNaoAplicadoSemInterfaces();
        testNetplanScriptFalhaGenerateRestaura();
        testManifestoAdulteradoNaoExecutaPkexec();
        testRevertNmUsaUuidComValoresEntreAspas();
        testValidatorRejeitaInjecaoIgnoreAutoDns();

        System.out.println();
        if (falhas == 0) {
            System.out.println("TODOS OS TESTES PASSARAM");
            System.exit(0);
        } else {
            System.out.println("FALHAS: " + falhas);
            System.exit(1);
        }
    }

    private static void testOsReleaseNixOs() {
        LinuxOsRelease rel = LinuxOsRelease.parse(List.of("ID=nixos", "NAME=\"NixOS\""));
        assertTrue("nixos id", rel.isNixOs());
        ok("testOsReleaseNixOs");
    }

    private static void testNmPreferidoSobreNetplan() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        Path osRelease = Files.createTempFile("os-release-", ".txt");
        Files.writeString(osRelease, "ID=ubuntu\n");
        Path netplan = Files.createTempDirectory("netplan-");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, netplan);
        InfoDistro ubuntu = new InfoDistro(TipoDistro.DEBIAN, "Ubuntu", "24.04", "NetworkManager");
        assertEq("backend", LinuxDnsBackend.NETWORK_MANAGER, selector.selecionar(ubuntu));
        ok("testNmPreferidoSobreNetplan");
    }

    private static void testParserEscapadoDoisPontos() {
        List<LinuxNmcliTerseParser.ConexaoAtiva> lista = LinuxNmcliTerseParser.parseConexoesAtivas(
            "Casa\\:Wi-Fi:802-11-wireless:wlp0s20f3");
        assertEq("nome ssid", "Casa:Wi-Fi", lista.get(0).nome());
        ok("testParserEscapadoDoisPontos");
    }

    private static void testFamiliesNaoConfunde11130() {
        assertTrue("nao detecta 11130", !LinuxDnsFamiliesDetector.textoContemFamilies("nameserver 1.1.1.30"));
        assertTrue("detecta 113", LinuxDnsFamiliesDetector.textoContemFamilies("DNS=1.1.1.3"));
        ok("testFamiliesNaoConfunde11130");
    }

    private static void testUmPkexecPorAplicarNm() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0, "Wi-Fi:802-11-wireless:wlan0", "", false));
        stubDnsShow(runner, "Wi-Fi");
        ConfiguradorDnsLinuxService svc = svc(runner, manifestTemp());
        InfoDistro debian = new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "NetworkManager");
        ConfiguracaoDns r = svc.configurar(debian);
        assertEq("um script pkexec", 1, runner.getScriptsPrivilegiados().size());
        if (r.isAplicado() || (r.getMensagem() != null && r.getMensagem().contains("verificação"))) {
            ok("testUmPkexecPorAplicarNm");
        } else {
            System.err.println("FALHA aplicado ou aviso verificacao: " + r.getMensagem());
            falhas++;
        }
    }

    private static void testScriptContemIpv6EIgnoreAutoDns() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0, "eth0:802-3-ethernet:enp0", "", false));
        stubDnsShow(runner, "eth0");
        ConfiguradorDnsLinuxService svc = svc(runner, manifestTemp());
        svc.configurar(new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "NetworkManager"));
        String script = runner.getScriptsPrivilegiados().get(0);
        assertTrue("ipv6.dns", script.contains("ipv6.dns"));
        assertTrue("ipv6 ignore", script.contains("ipv6.ignore-auto-dns 'yes'"));
        assertTrue("ipv4 ignore", script.contains("ipv4.ignore-auto-dns 'yes'"));
        ok("testScriptContemIpv6EIgnoreAutoDns");
    }

    private static void testResolvedRestartNoScript() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setComandoDisponivel("nmcli", false);
        runner.responder("systemctl is-active systemd-resolved", new LinuxCommandResult(0, "active", "", false));
        Path osRelease = Files.createTempFile("os-release-r-", ".txt");
        Files.writeString(osRelease, "ID=arch\n");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-np"));
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(
            runner, selector, new NixOsDnsSupport(runner, selector), new LinuxDnsManifestStore(manifestTemp()));
        svc.configurar(new InfoDistro(TipoDistro.ARCH, "Arch", "", "systemd-resolved"));
        String script = runner.getScriptsPrivilegiados().get(0);
        assertTrue("dropin fixo", script.contains(LinuxSystemPaths.DROPIN_RESOLVED));
        assertTrue("restart resolved", script.contains("systemctl restart systemd-resolved"));
        ok("testResolvedRestartNoScript");
    }

    private static void testPkexec126e127() {
        assertTrue("126 cancelado", LinuxPkexecMensagens.mensagemUsuario(126).contains("cancelou"));
        assertTrue("127 agente", LinuxPkexecMensagens.mensagemUsuario(127).contains("agente"));
        ok("testPkexec126e127");
    }

    private static void testRevertResolvedUmScript() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        Path manifest = manifestTemp();
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, "systemd-resolved");
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        new LinuxDnsManifestStore(manifest).salvar(props);
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner,
            Files.createTempFile("os-r", ".txt"), Path.of("/tmp/x"));
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(
            runner, selector, new NixOsDnsSupport(runner, selector), new LinuxDnsManifestStore(manifest));
        ConfiguracaoDns r = svc.desfazer();
        assertTrue("revert ok", r.isAplicado());
        assertEq("um script", 1, runner.getScriptsPrivilegiados().size());
        assertTrue("rm fixo", runner.getScriptsPrivilegiados().get(0).contains(LinuxSystemPaths.DROPIN_RESOLVED));
        assertTrue("manifesto removido", !new LinuxDnsManifestStore(manifest).existe());
        ok("testRevertResolvedUmScript");
    }

    private static void testMultiplasConexoesNoScript() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0,
                "Wi-Fi:802-11-wireless:wlan0\nEthernet:802-3-ethernet:enp0", "", false));
        stubDnsShow(runner, "Wi-Fi");
        stubDnsShow(runner, "Ethernet");
        ConfiguradorDnsLinuxService svc = svc(runner, manifestTemp());
        svc.configurar(new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "NetworkManager"));
        String script = runner.getScriptsPrivilegiados().get(0);
        assertTrue("wifi uuid", script.contains(UUID_WIFI));
        assertTrue("eth uuid", script.contains(UUID_ETH));
        assertTrue("rollback trap", script.contains("guialar_nm_rollback"));
        ok("testMultiplasConexoesNoScript");
    }

    private static void testNixOsComNmSemDnsGlobal() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("resolvectl status", new LinuxCommandResult(0, "Global\n  DNS Servers: -\n", "", false));
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0, "NixWifi:802-11-wireless:wlan0", "", false));
        stubDnsShow(runner, "NixWifi");
        Path osRelease = Files.createTempFile("os-nix-", ".txt");
        Files.writeString(osRelease, "ID=nixos\n");
        Path resolvStub = Files.createTempFile("resolv-", ".conf");
        Files.writeString(resolvStub, "nameserver 127.0.0.53\n");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-np"));
        NixOsDnsSupport nix = new NixOsDnsSupport(runner, selector, resolvStub);
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(runner, selector, nix,
            new LinuxDnsManifestStore(manifestTemp()));
        ConfiguracaoDns r = svc.configurar(new InfoDistro(TipoDistro.NIXOS, "NixOS", "", "NetworkManager"));
        assertTrue("nm script", !runner.getScriptsPrivilegiados().isEmpty());
        ok("testNixOsComNmSemDnsGlobal");
    }

    private static void testNixOsNmComDnsGlobalVaiParaSnippet() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("resolvectl status",
            new LinuxCommandResult(0, "Global\n  DNS Servers: 8.8.8.8\n", "", false));
        Path osRelease = Files.createTempFile("os-nix2-", ".txt");
        Files.writeString(osRelease, "ID=nixos\n");
        Path manifest = manifestTemp();
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-np"));
        NixOsDnsSupport nix = new NixOsDnsSupport(runner, selector);
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(runner, selector, nix,
            new LinuxDnsManifestStore(manifest));
        ConfiguracaoDns r = svc.configurar(new InfoDistro(TipoDistro.NIXOS, "NixOS", "", "NetworkManager"));
        assertTrue("aguardando", r.isAguardandoUsuario());
        assertTrue("sem pkexec", runner.getScriptsPrivilegiados().isEmpty());
        ok("testNixOsNmComDnsGlobalVaiParaSnippet");
    }

    private static void testStatusAguardandoPersisteNoManifesto() throws Exception {
        Path manifest = manifestTemp();
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_AGUARDANDO_NIXOS);
        props.setProperty(LinuxDnsManifestStore.KEY_NIXOS_SNIPPET, "networking.nameservers = [ ];");
        new LinuxDnsManifestStore(manifest).salvar(props);
        LinuxProtecaoStatusService status = new LinuxProtecaoStatusService(
            new LinuxDnsManifestStore(manifest), new VerificacaoDnsService());
        var a = status.avaliarSomenteManifesto();
        assertEq("status", StatusDns.AGUARDANDO_APLICACAO, a.status());
        assertTrue("snippet", status.carregarSnippetNixosPersistido().contains("nameservers"));
        ok("testStatusAguardandoPersisteNoManifesto");
    }

    private static void testRevertNaoApagaManifestoSeFalhar() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setResultadoScriptPadrao(new LinuxCommandResult(1, "", "erro", true));
        Path manifest = manifestTemp();
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, "systemd-resolved");
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        new LinuxDnsManifestStore(manifest).salvar(props);
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner,
            Files.createTempFile("os-f", ".txt"), Path.of("/tmp/x"));
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(
            runner, selector, new NixOsDnsSupport(runner, selector), new LinuxDnsManifestStore(manifest));
        ConfiguracaoDns r = svc.desfazer();
        assertTrue("falhou", !r.isAplicado());
        assertTrue("manifesto mantido", new LinuxDnsManifestStore(manifest).existe());
        ok("testRevertNaoApagaManifestoSeFalhar");
    }

    private static FakeLinuxCommandRunner runnerComNmAtivo() {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.responder("systemctl is-active NetworkManager", new LinuxCommandResult(0, "active", "", false));
        runner.responder("nmcli -t -f RUNNING general", new LinuxCommandResult(0, "running", "", false));
        return runner;
    }

    private static final String UUID_WIFI = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    private static final String UUID_ETH = "bbbbbbbb-cccc-dddd-eeee-ffffffffffff";

    private static void stubDnsShow(FakeLinuxCommandRunner runner, String nome) {
        String uuid = "Wi-Fi".equals(nome) ? UUID_WIFI : UUID_ETH;
        stubUuid(runner, nome, uuid);
        runner.responder("nmcli -t -f UUID,NAME connection show",
            new LinuxCommandResult(0, UUID_WIFI + ":Wi-Fi\n" + UUID_ETH + ":Ethernet", "", false));
        runner.responder("nmcli -t -f ipv4.dns,ipv6.dns,ipv4.ignore-auto-dns,ipv6.ignore-auto-dns connection show "
            + nome, new LinuxCommandResult(0,
            "ipv4.dns:8.8.8.8\nipv6.dns:\nipv4.ignore-auto-dns:no\nipv6.ignore-auto-dns:no", "", false));
    }

    private static void stubUuid(FakeLinuxCommandRunner runner, String nome, String uuid) {
        runner.responder("nmcli -t -g connection.uuid connection show " + nome,
            new LinuxCommandResult(0, uuid, "", false));
    }

    private static void testManifestoAdulteradoNaoExecutaPkexec() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f UUID,NAME connection show",
            new LinuxCommandResult(0, UUID_WIFI + ":Wi-Fi", "", false));
        Path manifest = manifestTemp();
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, "NetworkManager");
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        props.setProperty(LinuxNmManifestCodec.KEY_NM_COUNT, "1");
        props.setProperty("nm.0.uuid", UUID_WIFI);
        props.setProperty("nm.0.name", "Wi-Fi");
        props.setProperty("nm.0.ipv4.dns", "8.8.8.8");
        props.setProperty("nm.0.ipv6.dns", "");
        props.setProperty("nm.0.ipv4.ignore-auto-dns", "no; rm -rf /");
        props.setProperty("nm.0.ipv6.ignore-auto-dns", "no");
        new LinuxDnsManifestStore(manifest).salvar(props);
        ConfiguradorDnsLinuxService svc = svc(runner, manifest);
        ConfiguracaoDns r = svc.desfazer();
        assertTrue("falhou validacao", !r.isAplicado());
        assertTrue("sem pkexec", runner.getScriptsPrivilegiados().isEmpty());
        assertTrue("manifesto mantido", new LinuxDnsManifestStore(manifest).existe());
        ok("testManifestoAdulteradoNaoExecutaPkexec");
    }

    private static void testRevertNmUsaUuidComValoresEntreAspas() throws Exception {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f UUID,NAME connection show",
            new LinuxCommandResult(0, UUID_WIFI + ":Wi-Fi", "", false));
        Path manifest = manifestTemp();
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, "NetworkManager");
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        props.setProperty(LinuxNmManifestCodec.KEY_NM_COUNT, "1");
        props.setProperty("nm.0.uuid", UUID_WIFI);
        props.setProperty("nm.0.name", "Wi-Fi");
        props.setProperty("nm.0.ipv4.dns", "8.8.8.8");
        props.setProperty("nm.0.ipv6.dns", "");
        props.setProperty("nm.0.ipv4.ignore-auto-dns", "no");
        props.setProperty("nm.0.ipv6.ignore-auto-dns", "yes");
        new LinuxDnsManifestStore(manifest).salvar(props);
        ConfiguradorDnsLinuxService svc = svc(runner, manifest);
        ConfiguracaoDns r = svc.desfazer();
        assertTrue("revert ok", r.isAplicado());
        String script = runner.getScriptsPrivilegiados().get(0);
        assertTrue("uuid no script", script.contains(UUID_WIFI));
        assertTrue("ignore entre aspas", script.contains("ipv4.ignore-auto-dns 'no'")
            || script.contains("ipv4.ignore-auto-dns 'no'\n"));
        ok("testRevertNmUsaUuidComValoresEntreAspas");
    }

    private static void testValidatorRejeitaInjecaoIgnoreAutoDns() {
        FakeLinuxCommandRunner runner = runnerComNmAtivo();
        runner.responder("nmcli -t -f UUID,NAME connection show",
            new LinuxCommandResult(0, UUID_WIFI + ":Wi-Fi", "", false));
        LinuxNmManifestValidator v = new LinuxNmManifestValidator(runner);
        var r = v.validarParaRevert(List.of(new LinuxNmConexaoEstado(
            UUID_WIFI, "Wi-Fi", "1.1.1.1", "", "yes; echo hacked", "no")));
        assertTrue("invalido", !r.valido());
        ok("testValidatorRejeitaInjecaoIgnoreAutoDns");
    }

    private static void testExtrairInterfacesNetplan() {
        String yaml = """
            network:
              version: 2
              ethernets:
                enp0s3:
                  dhcp4: true
                "eth1":
                  nameservers:
                    addresses: [8.8.8.8]
              wifis:
                wlan0:
                  dhcp6: yes
            """;
        var r = LinuxNetplanSupport.extrairInterfacesNetplan(yaml);
        assertTrue("ethernets", r.containsKey("ethernets"));
        assertTrue("enp0s3 dhcp", r.get("ethernets").get("enp0s3").dhcp4());
        assertTrue("eth1 tem nameservers", r.get("ethernets").get("eth1").temNameservers());
        ok("testExtrairInterfacesNetplan");
    }

    private static void testGerarYamlNetplan() {
        var secoes = LinuxNetplanSupport.extrairInterfacesNetplan("""
            network:
              version: 2
              ethernets:
                enp0s3:
                  dhcp4: true
            """);
        String yaml = LinuxNetplanSupport.gerarYamlGuialar(secoes, ServidorDns.getPadrao());
        assertTrue("contem 99-guialar", yaml.contains("1.1.1.3"));
        assertTrue("dhcp4 override", yaml.contains("dhcp4-overrides"));
        ok("testGerarYamlNetplan");
    }

    private static void testSepararComDnsProprioNetplan() {
        var secoes = LinuxNetplanSupport.extrairInterfacesNetplan("""
            network:
              version: 2
              ethernets:
                eth0:
                  dhcp4: true
                eth1:
                  nameservers:
                    addresses: [1.1.1.1]
            """);
        var removidas = LinuxNetplanSupport.separarComDnsProprio(secoes);
        assertEq("removida eth1", List.of("eth1"), removidas);
        assertTrue("so eth0", secoes.get("ethernets").containsKey("eth0"));
        ok("testSepararComDnsProprioNetplan");
    }

    private static void testNetplanScriptBackupApplyRevert() {
        String apply = LinuxNetplanPrivilegedScript.buildApplyScript(ServidorDns.getPadrao());
        assertTrue("apply faz backup", apply.contains(LinuxSystemPaths.NETPLAN_GUAILAR_BACKUP));
        assertTrue("apply restaura no trap", apply.contains("restaurar_guialar"));
        assertTrue("apply valida generate", apply.contains("netplan generate"));
        assertTrue("apply lê YAML como root", apply.contains("python3"));
        String revert = LinuxNetplanPrivilegedScript.buildRevertScript();
        assertTrue("revert faz backup antes de remover", revert.contains("cp -a \"$GUAILAR\" \"$BACKUP\""));
        assertTrue("revert usa caminho fixo", revert.contains(LinuxSystemPaths.NETPLAN_GUAILAR));
        ok("testNetplanScriptBackupApplyRevert");
    }

    private static void testNetplanNaoAplicadoSemInterfaces() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setResultadoScriptPadrao(new LinuxCommandResult(0,
            "GUAILAR_STATUS=NO_INTERFACES\n", "", true));
        Path manifest = manifestTemp();
        ConfiguradorDnsLinuxService svc = svcNetplan(runner, manifest);
        InfoDistro debian = new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "Netplan");
        ConfiguracaoDns r = svc.configurar(debian);
        assertTrue("não aplicado", !r.isAplicado());
        assertTrue("sem manifesto", !Files.exists(manifest));
        ok("testNetplanNaoAplicadoSemInterfaces");
    }

    private static void testNetplanScriptFalhaGenerateRestaura() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setResultadoScriptPadrao(new LinuxCommandResult(1, "", "generate failed", true));
        Path manifest = manifestTemp();
        ConfiguradorDnsLinuxService svc = svcNetplan(runner, manifest);
        InfoDistro debian = new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "Netplan");
        ConfiguracaoDns r = svc.configurar(debian);
        assertTrue("falhou", !r.isAplicado());
        assertTrue("script contém restauração", !runner.getScriptsPrivilegiados().isEmpty()
            && runner.getScriptsPrivilegiados().get(0).contains("restaurar_guialar"));
        ok("testNetplanScriptFalhaGenerateRestaura");
    }

    private static ConfiguradorDnsLinuxService svcNetplan(FakeLinuxCommandRunner runner, Path manifest)
            throws Exception {
        Path osRelease = Files.createTempFile("os-deb-", ".txt");
        Files.writeString(osRelease, "ID=debian\n");
        Path netplan = Files.createTempDirectory("np-");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, netplan);
        runner.setComandoDisponivel("nmcli", false);
        return new ConfiguradorDnsLinuxService(runner, selector,
            new NixOsDnsSupport(runner, selector), new LinuxDnsManifestStore(manifest));
    }

    private static Path manifestTemp() throws Exception {
        Path p = Files.createTempFile("manifest-", ".properties");
        Files.delete(p);
        return p;
    }

    private static ConfiguradorDnsLinuxService svc(FakeLinuxCommandRunner runner, Path manifest) throws Exception {
        Path osRelease = Files.createTempFile("os-deb-", ".txt");
        Files.writeString(osRelease, "ID=debian\n");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-np"));
        return new ConfiguradorDnsLinuxService(runner, selector,
            new NixOsDnsSupport(runner, selector), new LinuxDnsManifestStore(manifest));
    }

    private static void assertEq(String nome, Object esperado, Object obtido) {
        if (esperado == null ? obtido != null : !esperado.equals(obtido)) {
            System.err.println("FALHA " + nome + ": esperado=" + esperado + " obtido=" + obtido);
            falhas++;
        }
    }

    private static void assertTrue(String nome, boolean cond) {
        if (!cond) {
            System.err.println("FALHA " + nome);
            falhas++;
        }
    }

    private static void ok(String nome) {
        System.out.println("  OK " + nome);
    }
}

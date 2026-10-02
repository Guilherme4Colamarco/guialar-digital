package br.uniube.pi.guialar.test;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.FakeLinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackend;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackendSelector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsFamiliesDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
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
        assertTrue("ipv6 ignore", script.contains("ipv6.ignore-auto-dns yes"));
        assertTrue("ipv4 ignore", script.contains("ipv4.ignore-auto-dns yes"));
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
        assertTrue("wifi", script.contains("'Wi-Fi'"));
        assertTrue("eth", script.contains("'Ethernet'"));
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

    private static void stubDnsShow(FakeLinuxCommandRunner runner, String nome) {
        runner.responder("nmcli -t -f ipv4.dns,ipv6.dns,ipv4.ignore-auto-dns,ipv6.ignore-auto-dns connection show "
            + nome, new LinuxCommandResult(0,
            "ipv4.dns:8.8.8.8\nipv6.dns:\nipv4.ignore-auto-dns:no\nipv6.ignore-auto-dns:no", "", false));
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

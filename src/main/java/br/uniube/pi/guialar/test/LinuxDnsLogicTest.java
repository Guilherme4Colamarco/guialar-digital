package br.uniube.pi.guialar.test;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.FakeLinuxCommandRunner;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxCommandResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackend;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsBackendSelector;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxOsRelease;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.NixOsDnsSupport;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsLinuxService;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Properties;

/**
 * Testes Linux com comandos mockados ({@code ant test-linux}).
 */
public class LinuxDnsLogicTest {

    private static int falhas = 0;

    public static void main(String[] args) throws Exception {
        System.out.println("=== GuiaLar Linux DNS logic tests ===");
        testOsReleaseNixOs();
        testNmPreferidoSobreNetplan();
        testPkexecSomenteNaEscritaDns();
        testRevertResolvedRemoveDropin();
        testNixOsComNmSemDnsGlobal();
        testNixOsSemNmRetornaSnippet();

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
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.responder("systemctl is-active NetworkManager",
            new LinuxCommandResult(0, "active", "", false));
        runner.responder("nmcli -t -f RUNNING general",
            new LinuxCommandResult(0, "running", "", false));

        Path osRelease = Files.createTempFile("os-release-", ".txt");
        Files.writeString(osRelease, "ID=ubuntu\nNAME=Ubuntu\n");
        Path netplan = Files.createTempDirectory("netplan-");

        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, netplan);
        InfoDistro ubuntu = new InfoDistro(TipoDistro.DEBIAN, "Ubuntu", "24.04", "NetworkManager");

        assertEq("backend", LinuxDnsBackend.NETWORK_MANAGER, selector.selecionar(ubuntu));
        ok("testNmPreferidoSobreNetplan");
    }

    private static void testPkexecSomenteNaEscritaDns() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.responder("systemctl is-active NetworkManager", new LinuxCommandResult(0, "active", "", false));
        runner.responder("nmcli -t -f RUNNING general", new LinuxCommandResult(0, "running", "", false));
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0, "Casa Wi-Fi:802-11-wireless:wlp0s20f3", "", false));
        runner.responder("nmcli -t -f ipv4.dns,ipv6.dns connection show Casa Wi-Fi",
            new LinuxCommandResult(0, "ipv4.dns:8.8.8.8\nipv6.dns:", "", false));

        runner.setFallback((chamada, fake) -> {
            if (chamada.privilegiado()) {
                return new LinuxCommandResult(0, "ok", "", true);
            }
            return new LinuxCommandResult(0, "", "", false);
        });

        Path manifest = Files.createTempFile("manifest-", ".properties");
        Files.delete(manifest);
        LinuxDnsManifestStore store = new LinuxDnsManifestStore(manifest);
        Path osRelease = Files.createTempFile("os-release-deb-", ".txt");
        Files.writeString(osRelease, "ID=debian\n");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/netplan-inexistente"));
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(
            runner, selector, new NixOsDnsSupport(runner, selector), store);

        InfoDistro debian = new InfoDistro(TipoDistro.DEBIAN, "Debian", "12", "NetworkManager");
        ConfiguracaoDns r = svc.configurar(debian);
        assertTrue("aplicado", r.isAplicado());

        boolean leituraSemPkexec = runner.getHistorico().stream()
            .filter(c -> !c.privilegiado())
            .anyMatch(c -> c.chave().contains("nmcli"));
        boolean escritaComPkexec = runner.getHistorico().stream()
            .filter(FakeLinuxCommandRunner.Chamada::privilegiado)
            .anyMatch(c -> c.chave().startsWith("nmcli connection modify"));

        assertTrue("leitura nmcli sem pkexec", leituraSemPkexec);
        assertTrue("escrita com pkexec", escritaComPkexec);
        ok("testPkexecSomenteNaEscritaDns");
    }

    private static void testRevertResolvedRemoveDropin() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setFallback((chamada, fake) -> new LinuxCommandResult(0, "", "", chamada.privilegiado()));

        Path manifest = Files.createTempFile("manifest-resolved-", ".properties");
        LinuxDnsManifestStore store = new LinuxDnsManifestStore(manifest);
        Properties props = new Properties();
        props.setProperty(LinuxDnsManifestStore.KEY_METODO, "systemd-resolved (drop-in)");
        props.setProperty(LinuxDnsManifestStore.KEY_MODO, LinuxDnsManifestStore.MODO_APLICADO);
        store.salvar(props);

        Path osRelease = Files.createTempFile("os-release-arch-", ".txt");
        Files.writeString(osRelease, "ID=arch\n");
        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-netplan"));
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(
            runner, selector, new NixOsDnsSupport(runner, selector), store);

        ConfiguracaoDns r = svc.desfazer();
        assertTrue("revert ok", r.isAplicado());
        assertTrue("mensagem dropin",
            r.getMensagem().contains("99-guialar.conf"));

        boolean rmDropin = runner.getHistorico().stream()
            .anyMatch(c -> c.privilegiado() && c.chave().contains("rm -f /etc/systemd/resolved.conf.d/99-guialar.conf"));
        assertTrue("rm dropin via pkexec", rmDropin);
        ok("testRevertResolvedRemoveDropin");
    }

    private static void testNixOsComNmSemDnsGlobal() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.responder("systemctl is-active NetworkManager", new LinuxCommandResult(0, "active", "", false));
        runner.responder("nmcli -t -f RUNNING general", new LinuxCommandResult(0, "running", "", false));
        runner.responder("resolvectl status", new LinuxCommandResult(0, "Global\n  DNS Servers: -\n", "", false));
        runner.responder("nmcli -t -f NAME,TYPE,DEVICE connection show --active",
            new LinuxCommandResult(0, "NixWifi:802-11-wireless:wlan0", "", false));
        runner.responder("nmcli -t -f ipv4.dns,ipv6.dns connection show NixWifi",
            new LinuxCommandResult(0, "ipv4.dns:\nipv6.dns:", "", false));
        runner.setFallback((chamada, fake) -> new LinuxCommandResult(0, "", "", chamada.privilegiado()));

        Path osRelease = Files.createTempFile("os-release-nix-", ".txt");
        Files.writeString(osRelease, "ID=nixos\nNAME=NixOS\n");
        Path manifest = Files.createTempFile("manifest-nix-nm-", ".properties");
        Files.delete(manifest);
        Path resolvStub = Files.createTempFile("resolv-stub-", ".conf");
        Files.writeString(resolvStub, "# stub\nnameserver 127.0.0.53\n");

        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-netplan"));
        NixOsDnsSupport nix = new NixOsDnsSupport(runner, selector, resolvStub);
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(runner, selector, nix,
            new LinuxDnsManifestStore(manifest));

        InfoDistro nixos = new InfoDistro(TipoDistro.NIXOS, "NixOS", "24.11", "NetworkManager");
        ConfiguracaoDns r = svc.configurar(nixos);
        if (!assertTrueSilent("nm aplicado no nixos", r.isAplicado())
            || !assertTrueSilent("usou nmcli modify", runner.getHistorico().stream()
                .anyMatch(c -> c.chave().contains("nmcli connection modify")))) {
            falhas++;
        } else {
            ok("testNixOsComNmSemDnsGlobal");
        }
    }

    private static void testNixOsSemNmRetornaSnippet() throws Exception {
        FakeLinuxCommandRunner runner = new FakeLinuxCommandRunner();
        runner.setComandoDisponivel("nmcli", false);
        runner.responder("resolvectl status", new LinuxCommandResult(0, "Global\n  DNS Servers: 8.8.8.8\n", "", false));

        Path osRelease = Files.createTempFile("os-release-nix2-", ".txt");
        Files.writeString(osRelease, "ID=nixos\n");
        Path manifest = Files.createTempFile("manifest-nix-snippet-", ".properties");
        Files.delete(manifest);

        LinuxDnsBackendSelector selector = new LinuxDnsBackendSelector(runner, osRelease, Path.of("/tmp/no-netplan"));
        NixOsDnsSupport nix = new NixOsDnsSupport(runner, selector);
        ConfiguradorDnsLinuxService svc = new ConfiguradorDnsLinuxService(runner, selector, nix,
            new LinuxDnsManifestStore(manifest));

        InfoDistro nixos = new InfoDistro(TipoDistro.NIXOS, "NixOS", "", "manual");
        ConfiguracaoDns r = svc.configurar(nixos);
        assertTrue("aguardando", r.isAguardandoUsuario());
        assertTrue("snippet nameservers", r.getTextoParaCopiar().contains("networking.nameservers"));
        assertTrue("sem aplicado", !r.isAplicado());
        ok("testNixOsSemNmRetornaSnippet");
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

    private static boolean assertTrueSilent(String nome, boolean cond) {
        if (!cond) {
            System.err.println("FALHA " + nome);
            return false;
        }
        return true;
    }

    private static void ok(String nome) {
        System.out.println("  OK " + nome);
    }
}

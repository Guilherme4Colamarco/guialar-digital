package br.uniube.pi.guialar.test;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.MockPowerShellExecutor;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.PowerShellResult;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsBrowserDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsChanger;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsUrlOpener;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Smoke/unit tests dos adapters Windows com PowerShell mockado.
 * Roda em Linux CI sem Windows real: {@code ant test-windows}.
 */
public class WindowsAdapterSmokeTest {

    private static int falhas = 0;

    public static void main(String[] args) throws Exception {
        System.out.println("=== GuiaLar Windows adapter smoke tests ===");
        testParseAdaptadores();
        testManifestoRoundTrip();
        testDnsSemAdminNaoAltera();
        testDnsUacNegado();
        testDnsAplicadoSalvaManifesto();
        testDesfazerSemManifesto();
        testBrowserDetectorCaminhos();
        testGuiasNavegador();
        testScriptContemSetDns();

        System.out.println();
        if (falhas == 0) {
            System.out.println("TODOS OS TESTES PASSARAM");
            System.exit(0);
        } else {
            System.out.println("FALHAS: " + falhas);
            System.exit(1);
        }
    }

    private static void testParseAdaptadores() {
        String stdout = """
            ALIAS=Wi-Fi|INDEX=12|IPV4=8.8.8.8,8.8.4.4|IPV6=2001:4860:4860::8888|DHCP=true
            ALIAS=Ethernet|INDEX=5|IPV4=1.1.1.1|IPV6=|DHCP=false
            """;
        List<WindowsDnsManifestStore.EstadoAdaptador> lista =
            WindowsDnsChanger.parseAdaptadores(stdout);
        assertEq("parse count", 2, lista.size());
        assertEq("alias wifi", "Wi-Fi", lista.get(0).alias());
        assertEq("index wifi", 12, lista.get(0).interfaceIndex());
        assertEq("ipv4 count", 2, lista.get(0).ipv4Anterior().size());
        assertEq("ethernet dhcp", false, lista.get(1).eraDhcp());
        ok("testParseAdaptadores");
    }

    private static void testManifestoRoundTrip() throws Exception {
        Path tmp = Files.createTempFile("guialar-manifest-", ".properties");
        try {
            WindowsDnsManifestStore store = new WindowsDnsManifestStore(tmp);
            var mapa = java.util.Map.of(
                "Wi-Fi", new WindowsDnsManifestStore.EstadoAdaptador(
                    "Wi-Fi", 12, List.of("8.8.8.8"), List.of(), true)
            );
            var original = WindowsDnsManifestStore.ManifestoDns.criar(
                WindowsDnsChanger.METODO, "1.1.1.3,1.0.0.3", mapa);
            store.salvar(original);
            var carregado = store.carregar();
            assertTrue("existe", store.existe());
            assertEq("metodo", WindowsDnsChanger.METODO, carregado.metodo());
            assertEq("servidor", "1.1.1.3,1.0.0.3", carregado.servidorAlvo());
            assertEq("adapters", 1, carregado.adaptadores().size());
            assertEq("prev dns", "8.8.8.8", carregado.adaptadores().get("Wi-Fi").ipv4Anterior().get(0));
            store.limpar();
            assertTrue("limpo", !store.existe());
            ok("testManifestoRoundTrip");
        } finally {
            Files.deleteIfExists(tmp);
        }
    }

    private static void testDnsSemAdminNaoAltera() throws Exception {
        Path tmp = Files.createTempFile("guialar-m-", ".properties");
        Files.deleteIfExists(tmp);
        AtomicBoolean escritaElevada = new AtomicBoolean(false);
        MockPowerShellExecutor mock = new MockPowerShellExecutor((script, elev) -> {
            if (scriptContains(script, "IsInRole") || scriptContains(script, "EnableLUA")) {
                return new PowerShellResult(0, "USER", "");
            }
            if (scriptContains(script, "ALIAS=") || scriptContains(script, "Get-NetAdapter")
                    || scriptContains(script, "Get-DnsClientServerAddress")) {
                return new PowerShellResult(0,
                    "ALIAS=Wi-Fi|INDEX=12|IPV4=8.8.8.8|IPV6=|DHCP=true", "");
            }
            if (scriptContains(script, "Set-DnsClientServerAddress")) {
                if (elev) {
                    escritaElevada.set(true);
                }
                return PowerShellResult.elevacaoNegada("UAC denied");
            }
            return new PowerShellResult(0, "USER", "");
        });

        WindowsDnsChanger changer = new WindowsDnsChanger(mock, new WindowsDnsManifestStore(tmp));
        ConfiguracaoDns r = changer.configurar(ServidorDns.getPadrao());
        assertTrue("nao aplicado", !r.isAplicado());
        assertTrue("mensagem pt", r.getMensagem().toLowerCase().contains("não aplicado")
            || r.getMensagem().toLowerCase().contains("nao aplicado")
            || r.getMensagem().contains("Elevação")
            || r.getMensagem().contains("elevação"));
        assertTrue("sem manifesto", !Files.exists(tmp) || Files.size(tmp) == 0 || !new WindowsDnsManifestStore(tmp).existe());
        Files.deleteIfExists(tmp);
        ok("testDnsSemAdminNaoAltera");
    }

    private static void testDnsUacNegado() throws Exception {
        Path tmp = Files.createTempFile("guialar-m2-", ".properties");
        Files.deleteIfExists(tmp);
        MockPowerShellExecutor mock = new MockPowerShellExecutor((script, elev) -> {
            if (scriptContains(script, "IsInRole")) {
                return new PowerShellResult(0, "USER", "");
            }
            if (scriptContains(script, "Get-NetAdapter") || scriptContains(script, "ALIAS=")) {
                return new PowerShellResult(0,
                    "ALIAS=Ethernet|INDEX=3|IPV4=9.9.9.9|IPV6=|DHCP=true", "");
            }
            if (elev || scriptContains(script, "Set-DnsClientServerAddress")) {
                return PowerShellResult.elevacaoNegada("cancelado");
            }
            return new PowerShellResult(0, "", "");
        });
        WindowsDnsChanger changer = new WindowsDnsChanger(mock, new WindowsDnsManifestStore(tmp));
        ConfiguracaoDns r = changer.configurar(ServidorDns.getPadrao());
        assertTrue("falhou graciosamente", !r.isAplicado());
        assertTrue("explica uac", r.getMensagem().toLowerCase().contains("uac")
            || r.getMensagem().toLowerCase().contains("elev"));
        assertTrue("inalterado", !new WindowsDnsManifestStore(tmp).existe());
        Files.deleteIfExists(tmp);
        ok("testDnsUacNegado");
    }

    private static void testDnsAplicadoSalvaManifesto() throws Exception {
        Path tmp = Files.createTempFile("guialar-m3-", ".properties");
        MockPowerShellExecutor mock = new MockPowerShellExecutor((script, elev) -> {
            if (scriptContains(script, "IsInRole")) {
                return new PowerShellResult(0, "ELEVATED", "");
            }
            if (scriptContains(script, "Get-NetAdapter") || scriptContains(script, "ALIAS=")) {
                return new PowerShellResult(0,
                    "ALIAS=Wi-Fi|INDEX=12|IPV4=8.8.8.8|IPV6=|DHCP=true", "");
            }
            if (scriptContains(script, "Set-DnsClientServerAddress")) {
                return new PowerShellResult(0, "APPLIED=1", "");
            }
            return new PowerShellResult(0, "", "");
        });
        WindowsDnsManifestStore store = new WindowsDnsManifestStore(tmp);
        WindowsDnsChanger changer = new WindowsDnsChanger(mock, store);
        ConfiguracaoDns r = changer.configurar(ServidorDns.getPadrao());
        assertTrue("aplicado", r.isAplicado());
        assertTrue("manifesto", store.existe());
        assertEq("alias salvo", "Wi-Fi", store.carregar().aliases().get(0));
        Files.deleteIfExists(tmp);
        ok("testDnsAplicadoSalvaManifesto");
    }

    private static void testDesfazerSemManifesto() throws Exception {
        Path tmp = Files.createTempFile("guialar-m4-", ".properties");
        Files.deleteIfExists(tmp);
        MockPowerShellExecutor mock = new MockPowerShellExecutor(
            (s, e) -> new PowerShellResult(0, "ELEVATED", ""));
        WindowsDnsChanger changer = new WindowsDnsChanger(mock, new WindowsDnsManifestStore(tmp));
        ConfiguracaoDns r = changer.reverter();
        assertTrue("nada a desfazer", !r.isAplicado());
        assertTrue("mensagem", r.getMensagem().toLowerCase().contains("manifesto")
            || r.getMensagem().toLowerCase().contains("desfazer"));
        ok("testDesfazerSemManifesto");
    }

    private static void testBrowserDetectorCaminhos() throws Exception {
        Path root = Files.createTempDirectory("guialar-browsers");
        Path edge = root.resolve(Path.of("Microsoft", "Edge", "Application"));
        Files.createDirectories(edge);
        Path edgeExe = edge.resolve("msedge.exe");
        Files.writeString(edgeExe, "fake");

        MockPowerShellExecutor mock = new MockPowerShellExecutor(
            (s, e) -> new PowerShellResult(0, "", ""));
        WindowsBrowserDetector det = new WindowsBrowserDetector(
            mock, root.toString(), root.toString(), root.toString(), root.toString());

        List<Navegador> navs = det.detectar();
        boolean achouEdge = navs.stream().anyMatch(n -> n.getNome().contains("Edge"));
        assertTrue("detectou edge por caminho", achouEdge);
        ok("testBrowserDetectorCaminhos");
    }

    private static void testGuiasNavegador() {
        List<String> brave = WindowsUrlOpener.guiasPara("Brave");
        assertTrue("brave shields", brave.stream().anyMatch(s -> s.toLowerCase().contains("shield")));
        List<String> ff = WindowsUrlOpener.guiasPara("Mozilla Firefox");
        assertTrue("firefox ublock", ff.stream().anyMatch(s -> s.toLowerCase().contains("ublock")));
        List<String> edge = WindowsUrlOpener.guiasPara("Microsoft Edge");
        assertTrue("edge doh", edge.stream().anyMatch(s -> s.toLowerCase().contains("doh")
            || s.toLowerCase().contains("dns")));
        ok("testGuiasNavegador");
    }

    private static void testScriptContemSetDns() {
        var adaptador = new WindowsDnsManifestStore.EstadoAdaptador(
            "Wi-Fi", 12, List.of("8.8.8.8"), List.of(), true);
        String script = WindowsDnsChanger.scriptAplicarDns(
            List.of(adaptador), ServidorDns.getPadrao());
        assertTrue("tem Set-Dns", script.contains("Set-DnsClientServerAddress"));
        assertTrue("tem 1.1.1.3", script.contains("1.1.1.3"));
        assertTrue("tem 1.0.0.3", script.contains("1.0.0.3"));
        assertTrue("tem ipv6", script.contains("2606:4700:4700::1113"));
        ok("testScriptContemSetDns");
    }

    // --- asserts ---

    private static boolean scriptContains(String script, String token) {
        return script != null && script.contains(token);
    }

    private static void assertEq(String nome, Object expected, Object actual) {
        if (expected == null ? actual != null : !expected.equals(actual)) {
            falhas++;
            System.out.println("FAIL " + nome + ": expected=" + expected + " actual=" + actual);
        }
    }

    private static void assertTrue(String nome, boolean cond) {
        if (!cond) {
            falhas++;
            System.out.println("FAIL " + nome);
        }
    }

    private static void ok(String nome) {
        System.out.println("OK   " + nome);
    }
}

package br.uniube.pi.guialar.aplicacao.diagnostico;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsBrowserDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsChanger;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsElevationProbe;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsPaths;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.PowerShellExecutor;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.ProcessPowerShellExecutor;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorSistemaService;
import br.uniube.pi.guialar.dominio.diagnostico.DiagnosticoAmbiente;
import br.uniube.pi.guialar.dominio.diagnostico.StatusDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

import java.io.IOException;
import java.net.InetAddress;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;

/**
 * Diagnóstico somente leitura do ambiente (Linux ou Windows).
 * Não exige admin; nunca altera o sistema.
 */
public class DiagnosticoAmbienteService {

    private final DetectorSistemaService detectorSistema;
    private final PowerShellExecutor powerShell;
    private final WindowsDnsChanger windowsDns;
    private final WindowsBrowserDetector windowsBrowsers;
    private final WindowsElevationProbe elevation;
    private final WindowsDnsManifestStore manifestoStore;
    private final DetectorNavegadorService detectorNavegador;

    public DiagnosticoAmbienteService() {
        this(new ProcessPowerShellExecutor());
    }

    public DiagnosticoAmbienteService(PowerShellExecutor powerShell) {
        this.detectorSistema = new DetectorSistemaService();
        this.powerShell = powerShell;
        this.manifestoStore = new WindowsDnsManifestStore();
        this.windowsDns = new WindowsDnsChanger(powerShell, manifestoStore);
        this.windowsBrowsers = new WindowsBrowserDetector(powerShell);
        this.elevation = new WindowsElevationProbe(powerShell);
        this.detectorNavegador = new DetectorNavegadorService();
    }

    public DiagnosticoAmbiente diagnosticar() {
        TipoSistema tipo = detectorSistema.detectar();
        if (tipo.isWindows()) {
            return diagnosticarWindows();
        }
        return diagnosticarLinux(tipo);
    }

    private DiagnosticoAmbiente diagnosticarWindows() {
        List<String> oQueFunciona = new ArrayList<>();
        oQueFunciona.add("Abrir o GuiaLar e ver este diagnóstico (sem admin)");
        oQueFunciona.add("Detectar navegadores e abrir guias (uBlock / Shields / DoH)");
        oQueFunciona.add("Salvar dados em " + WindowsPaths.diretorioDados());

        boolean elevado = elevation.isProcessoElevado();
        boolean podeElevar = elevation.podeSolicitarElevacao();

        List<String> dnsAtuais = windowsDns.listarServidoresDnsAtuais();
        String motivoLeitura = windowsDns.motivoFalhaLeituraDns();
        StatusDns status;
        String notaDns;

        ServidorDns alvo = ServidorDns.getPadrao();
        boolean manifestoExiste = manifestoStore.existe();
        boolean dnsCorresponde = dnsAtuais.contains(alvo.getPrimario())
            || dnsAtuais.contains(alvo.getSecundario());

        if (motivoLeitura != null && dnsAtuais.isEmpty()) {
            status = StatusDns.LEITURA_BLOQUEADA;
            notaDns = motivoLeitura;
        } else if (dnsCorresponde && manifestoExiste) {
            status = StatusDns.APLICADO;
            notaDns = "Cloudflare Families detectado e manifesto presente.";
        } else if (dnsCorresponde) {
            status = StatusDns.PARCIAL;
            notaDns = "DNS Families parece presente, mas sem manifesto GuiaLar "
                + "(pode ter sido configurado manualmente ou por GPO).";
        } else if (manifestoExiste) {
            status = StatusDns.NAO_APLICADO;
            notaDns = "Há manifesto, porém o DNS atual não corresponde ao Cloudflare Families. "
                + "Filtro GuiaLar: não aplicado / sobrescrito.";
        } else {
            status = StatusDns.NAO_APLICADO;
            notaDns = "DNS GuiaLar não aplicado. Em conta limitada a aplicação "
                + "pode falhar no UAC/GPO — isso é esperado em laboratório.";
        }

        if (status != StatusDns.APLICADO) {
            // Nunca alegar filtro ativo se não aplicado
            oQueFunciona.add("Guias manuais de DoH/adblocker mesmo sem DNS do sistema");
        }

        List<Navegador> navegadores = windowsBrowsers.detectar();
        List<String> conectividade = testarConectividadeBasica();

        DiagnosticoAmbiente diag = DiagnosticoAmbiente.builder()
            .tipoSistema(TipoSistema.WINDOWS)
            .nomeSistema(System.getProperty("os.name", "Windows"))
            .versaoSistema(obterVersaoWindows())
            .arquitetura(System.getProperty("os.arch", "N/A"))
            .processoElevado(elevado)
            .podeElevar(podeElevar)
            .statusDns(status)
            .servidoresDnsAtuais(dnsAtuais)
            .notaDns(notaDns)
            .navegadores(navegadores)
            .notasConectividade(conectividade)
            .oQueFunciona(oQueFunciona)
            .caminhoDadosUsuario(WindowsPaths.diretorioDados().toString())
            .build();

        salvarRelatorio(diag);
        return diag;
    }

    private DiagnosticoAmbiente diagnosticarLinux(TipoSistema tipo) {
        List<String> oQueFunciona = List.of(
            "Detecção de distribuição e navegadores",
            "Diagnóstico sem root",
            "Aplicação de DNS apenas com privilégios (pkexec/sudo)"
        );
        List<Navegador> navegadores = detectorNavegador.detectar();
        return DiagnosticoAmbiente.builder()
            .tipoSistema(tipo)
            .nomeSistema(System.getProperty("os.name", "Linux"))
            .versaoSistema(System.getProperty("os.version", "N/A"))
            .arquitetura(System.getProperty("os.arch", "N/A"))
            .processoElevado("root".equals(System.getProperty("user.name")))
            .podeElevar(true)
            .statusDns(StatusDns.DESCONHECIDO)
            .notaDns("No Linux use a configuração via NetworkManager/resolved (ver CLI completa).")
            .navegadores(navegadores)
            .notasConectividade(testarConectividadeBasica())
            .oQueFunciona(oQueFunciona)
            .caminhoDadosUsuario(System.getProperty("user.home") + "/.guialar")
            .build();
    }

    private String obterVersaoWindows() {
        PowerShellResultWrapper r = new PowerShellResultWrapper(powerShell.executar(
            "(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion').ProductName + ' ' + "
                + "(Get-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion').DisplayVersion"));
        if (r.ok && !r.stdout.isBlank()) {
            return r.stdout.trim();
        }
        return System.getProperty("os.version", "N/A");
    }

    private List<String> testarConectividadeBasica() {
        List<String> notas = new ArrayList<>();
        try {
            boolean ok = InetAddress.getByName("one.one.one.one").isReachable(2000)
                || InetAddress.getByName("1.1.1.3").isReachable(2000);
            if (ok) {
                notas.add("Resolução/alcance básico a Cloudflare parece OK (best-effort).");
            } else {
                notas.add("Não confirmou alcance ICMP a 1.1.1.3 (comum em redes filtradas; não é erro fatal).");
            }
        } catch (Exception e) {
            notas.add("Rede filtrada ou sem Internet: o app ainda abre; DNS/guias podem ser manuais. ("
                + e.getClass().getSimpleName() + ")");
        }
        notas.add("Rede de campus filtrada: o app não precisa mudar DNS para o diagnóstico passar.");
        return notas;
    }

    private void salvarRelatorio(DiagnosticoAmbiente diag) {
        try {
            Files.createDirectories(WindowsPaths.diretorioDados());
            Files.writeString(WindowsPaths.logDiagnostico(), diag.formatarRelatorio());
        } catch (IOException ignored) {
            // best-effort
        }
    }

    /** Evita dependência circular de import no método privado. */
    private record PowerShellResultWrapper(boolean ok, String stdout) {
        PowerShellResultWrapper(br.uniube.pi.guialar.aplicacao.adaptadores.windows.PowerShellResult r) {
            this(r.isOk(), r.getStdout());
        }
    }
}

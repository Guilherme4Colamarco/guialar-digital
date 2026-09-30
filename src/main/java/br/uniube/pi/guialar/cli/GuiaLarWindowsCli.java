package br.uniube.pi.guialar.cli;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsUrlOpener;
import br.uniube.pi.guialar.aplicacao.diagnostico.DiagnosticoAmbienteService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsWindowsService;
import br.uniube.pi.guialar.dominio.diagnostico.DiagnosticoAmbiente;
import br.uniube.pi.guialar.dominio.diagnostico.StatusDns;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;

import java.util.Arrays;
import java.util.List;
import java.util.Scanner;

/**
 * Fluxo CLI para Windows: diagnóstico sem admin, DNS com UAC sob demanda,
 * guias de navegador e desfazer via manifesto.
 */
public class GuiaLarWindowsCli {

    private final DiagnosticoAmbienteService diagnosticoService;
    private final ConfiguradorDnsWindowsService dnsService;
    private final WindowsUrlOpener urlOpener;

    public GuiaLarWindowsCli() {
        this(new DiagnosticoAmbienteService(), new ConfiguradorDnsWindowsService(), new WindowsUrlOpener());
    }

    public GuiaLarWindowsCli(DiagnosticoAmbienteService diagnosticoService,
                             ConfiguradorDnsWindowsService dnsService,
                             WindowsUrlOpener urlOpener) {
        this.diagnosticoService = diagnosticoService;
        this.dnsService = dnsService;
        this.urlOpener = urlOpener;
    }

    public void run(String... args) {
        List<String> argList = Arrays.asList(args);
        boolean soDiagnostico = argList.contains("--diagnostico") || argList.contains("--diagnose");
        boolean desfazer = argList.contains("--desfazer") || argList.contains("--undo");
        boolean aplicar = argList.contains("--aplicar-dns") || argList.contains("--apply-dns");
        boolean abrirGuias = argList.contains("--abrir-guias") || argList.contains("--open-guides");

        imprimirCabecalho();

        DiagnosticoAmbiente diag = diagnosticoService.diagnosticar();
        System.out.println(diag.formatarRelatorio());
        System.out.println();

        if (soDiagnostico) {
            imprimirExpectativaLaboratorio(diag);
            return;
        }

        if (desfazer) {
            ConfiguracaoDns r = dnsService.desfazer();
            System.out.println(r.isAplicado() ? "✓ " + r.getMensagem() : "○ " + r.getMensagem());
            return;
        }

        exibirGuiasNavegadores(diag.getNavegadores(), abrirGuias);

        if (aplicar || confirmar("Deseja tentar aplicar o DNS Cloudflare Families agora? (pode pedir UAC)")) {
            System.out.println();
            ConfiguracaoDns resultado = dnsService.configurar(ServidorDns.getPadrao());
            if (!resultado.isAplicado()) {
                System.out.println();
                System.out.println("Status final: DNS " + StatusDns.NAO_APLICADO.getRotuloPt() + ".");
                System.out.println("O filtro do sistema NÃO está ativo via GuiaLar neste PC.");
                System.out.println("Continue com os guias de navegador (DoH + uBlock/Shields).");
            } else {
                System.out.println();
                System.out.println("Status final: DNS " + StatusDns.APLICADO.getRotuloPt() + ".");
                System.out.println("Ainda configure DoH nos navegadores para " + ServidorDns.DOH_ENDPOINT);
            }
        } else {
            System.out.println("Aplicação de DNS ignorada. Diagnóstico e guias permanecem disponíveis.");
        }

        imprimirExpectativaLaboratorio(diag);
    }

    private void exibirGuiasNavegadores(List<Navegador> navegadores, boolean abrir) {
        System.out.println("=== Orientação de navegadores (Windows) ===");
        if (navegadores.isEmpty()) {
            System.out.println("Nenhum navegador detectado. Instale Edge/Chrome/Firefox/Brave se possível.");
            System.out.println();
            return;
        }
        for (Navegador nav : navegadores) {
            System.out.println("• " + nav.getNome() + ":");
            for (String linha : WindowsUrlOpener.guiasPara(nav.getNome())) {
                System.out.println("    " + linha);
            }
            if (abrir) {
                String url = WindowsUrlOpener.urlLojaPadrao(nav.getNome());
                boolean ok = urlOpener.abrir(url);
                System.out.println("    Abrir link: " + (ok ? "ok" : "falhou") + " → " + url);
            }
            System.out.println();
        }
    }

    private void imprimirExpectativaLaboratorio(DiagnosticoAmbiente diag) {
        System.out.println("=== Smoke test em PC de laboratório ===");
        System.out.println("• Abrir o app e ver diagnóstico: esperado OK sem admin.");
        System.out.println("• Aplicar DNS sem admin/UAC/GPO: esperado \""
            + StatusDns.NAO_APLICADO.getRotuloPt() + "\" com mensagem clara (sem crash).");
        System.out.println("• Status DNS atual neste relatório: " + diag.getStatusDns().getRotuloPt());
        if (diag.getStatusDns() != StatusDns.APLICADO) {
            System.out.println("• Não afirmamos que o filtro de sistema está ativo.");
        }
        System.out.println();
    }

    private boolean confirmar(String pergunta) {
        System.out.print(pergunta + " [s/N]: ");
        try {
            Scanner sc = new Scanner(System.in);
            if (!sc.hasNextLine()) {
                return false;
            }
            String linha = sc.nextLine().trim().toLowerCase();
            return linha.equals("s") || linha.equals("sim") || linha.equals("y") || linha.equals("yes");
        } catch (Exception e) {
            return false;
        }
    }

    private void imprimirCabecalho() {
        System.out.println("╔════════════════════════════════════════════════════════════════╗");
        System.out.println("║           GuiaLar Digital - Windows (smoke test)               ║");
        System.out.println("║     Diagnóstico sem admin · DNS via PowerShell (UAC)           ║");
        System.out.println("╚════════════════════════════════════════════════════════════════╝");
        System.out.println();
        System.out.println("Flags: --diagnostico  --aplicar-dns  --desfazer  --abrir-guias");
        System.out.println();
    }
}

package br.uniube.pi.guialar;

import br.uniube.pi.guialar.cli.GuiaLarCli;
import br.uniube.pi.guialar.cli.GuiaLarWindowsCli;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorSistemaService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.gui.GuiaLarGui;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

import java.awt.GraphicsEnvironment;
import java.util.Arrays;
import java.util.List;

/**
 * GuiaLar Digital - Assistente para configuração de DNS seguro e adblockers.
 *
 * Plataformas: Linux (completo) + Windows (diagnóstico + DNS via PowerShell).
 *
 * @author Guilherme Amaral Colamarco Resende de Melo
 * @version 0.2.0
 */
public class GuiaLarApplication {

    public static void main(String[] args) {
        if (deveUsarGui(args)) {
            GuiaLarGui.iniciar();
            return;
        }

        executarCli(args);
    }

    /**
     * Decide entre interface gráfica e CLI.
     *
     * - "--gui" força a GUI; "--cli" força a CLI.
     * - Sem flags: usa a GUI quando há ambiente gráfico disponível,
     *   caindo para a CLI em ambientes headless (servidores, CI, containers).
     */
    private static boolean deveUsarGui(String[] args) {
        boolean forcarCli = Arrays.asList(args).contains("--cli");
        boolean forcarGui = Arrays.asList(args).contains("--gui");

        if (forcarCli) {
            return false;
        }
        if (forcarGui) {
            return true;
        }
        // Flags de operação CLI no Windows não devem abrir GUI
        if (Arrays.asList(args).contains("--diagnostico")
                || Arrays.asList(args).contains("--diagnose")
                || Arrays.asList(args).contains("--desfazer")
                || Arrays.asList(args).contains("--undo")
                || Arrays.asList(args).contains("--aplicar-dns")
                || Arrays.asList(args).contains("--apply-dns")) {
            return false;
        }
        return !GraphicsEnvironment.isHeadless();
    }

    private static void executarCli(String[] args) {
        DetectorSistemaService detectorSistema = new DetectorSistemaService();
        TipoSistema sistema = detectorSistema.detectar();

        if (sistema == TipoSistema.DESCONHECIDO) {
            System.err.println("❌ Sistema operacional não suportado.");
            System.err.println("   O GuiaLar Digital suporta Linux (Debian/Fedora/Arch) e Windows.");
            System.exit(1);
        }

        List<String> argList = Arrays.asList(args);
        boolean soDiagnostico = argList.contains("--diagnostico") || argList.contains("--diagnose");

        if (soDiagnostico) {
            try {
                var diag = new br.uniube.pi.guialar.aplicacao.diagnostico.DiagnosticoAmbienteService()
                    .diagnosticar();
                System.out.println(diag.formatarRelatorio());
                if (diag.getStatusDns() != br.uniube.pi.guialar.dominio.diagnostico.StatusDns.APLICADO) {
                    System.out.println("Filtro de sistema: não afirmado como ativo (status: "
                        + diag.getStatusDns().getRotuloPt() + ").");
                }
            } catch (Exception e) {
                System.err.println("Falha no diagnóstico (sistema inalterado): " + e.getMessage());
                System.exit(1);
            }
            return;
        }

        if (sistema.isWindows()) {
            try {
                new GuiaLarWindowsCli().run(args);
            } catch (Exception e) {
                System.err.println("\n❌ Erro no fluxo Windows (sem alterar o sistema): " + e.getMessage());
                e.printStackTrace();
                System.exit(1);
            }
            return;
        }

        DetectorDistroService detectorDistro = new DetectorDistroService();
        DetectorNavegadorService detectorNavegador = new DetectorNavegadorService();
        ConfiguradorDnsService configuradorDns = new ConfiguradorDnsService();
        InstaladorExtensaoService instaladorExtensao = new InstaladorExtensaoService();
        AutorizadorService autorizador = new AutorizadorService();
        VerificacaoDnsService verificacaoDns = new VerificacaoDnsService();

        GuiaLarCli cli = new GuiaLarCli(
            detectorDistro,
            detectorNavegador,
            configuradorDns,
            instaladorExtensao,
            autorizador,
            verificacaoDns
        );

        try {
            cli.run(args);
        } catch (Exception e) {
            System.err.println("\n❌ Erro fatal: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }
}

package br.uniube.pi.guialar;

import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.aplicacao.plataforma.PlataformaFactory;
import br.uniube.pi.guialar.aplicacao.plataforma.PlataformaService;
import br.uniube.pi.guialar.cli.GuiaLarCli;
import br.uniube.pi.guialar.gui.GuiaLarGui;

import java.awt.GraphicsEnvironment;
import java.util.Arrays;

/**
 * GuiaLar Digital - Assistente para configuração de DNS seguro e adblockers.
 *
 * Multiplataforma: Linux (Debian/Fedora/Arch) e Windows (10/11).
 *
 * Todo o comportamento específico de cada sistema fica em
 * {@link PlataformaService}, escolhido em tempo de execução por
 * {@link PlataformaFactory}. A GUI (Swing) e a CLI consomem essa mesma
 * abstração.
 *
 * @author Guilherme Amaral Colamarco Resende de Melo
 * @version 0.1.0
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
        return !GraphicsEnvironment.isHeadless();
    }

    private static void executarCli(String[] args) {
        PlataformaService plataforma = PlataformaFactory.criar();
        GuiaLarCli cli = new GuiaLarCli(plataforma, new AutorizadorService());

        try {
            cli.run(args);
        } catch (Exception e) {
            System.err.println("\n❌ Erro fatal: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }
}

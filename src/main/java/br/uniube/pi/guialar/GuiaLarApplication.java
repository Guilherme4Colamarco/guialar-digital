package br.uniube.pi.guialar;

import br.uniube.pi.guialar.cli.GuiaLarCli;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorSistemaService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsWindowsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

/**
 * GuiaLar Digital - Assistente para configuração de DNS seguro e adblockers.
 * 
 * MVP Linux: Debian, Fedora e Arch Linux
 * 
 * Funcionalidades:
 * 1. Mudar DNS do sistema para Cloudflare 1.1.1.1 for Families
 *    - IPv4: 1.1.1.3 / 1.0.0.3
 *    - IPv6: 2606:4700:4700::1113 / 2606:4700:4700::1003
 * 2. Detectar navegadores instalados (Firefox vs Chromium)
 * 3. Instalar adblocker (uBlock Origin / uBlock Origin Lite)
 * 
 * Segurança:
 * - Sempre exibe plano de ações antes de executar
 * - Solicita autorização via polkit/pkexec
 * - Faz backup antes de modificar configurações
 * - Sem coleta de histórico de navegação
 * 
 * @author Guilherme Amaral Colamarco Resende de Melo
 * @version 0.1.0
 */
public class GuiaLarApplication {

    public static void main(String[] args) {
        DetectorSistemaService detectorSistema = new DetectorSistemaService();
        TipoSistema sistema = detectorSistema.detectar();

        if (sistema == TipoSistema.DESCONHECIDO) {
            System.err.println("❌ Sistema operacional não suportado.");
            System.err.println("   O GuiaLar Digital suporta Linux (Debian/Fedora/Arch) e Windows.");
            System.exit(1);
        }

        if (sistema.isWindows()) {
            System.out.println("⚠️  Windows detectado - Esboço básico disponível");
            System.out.println();
            ConfiguradorDnsWindowsService windowsService = new ConfiguradorDnsWindowsService();
            windowsService.configurar();
            System.exit(0);
        }

        DetectorDistroService detectorDistro = new DetectorDistroService();
        DetectorNavegadorService detectorNavegador = new DetectorNavegadorService();
        ConfiguradorDnsService configuradorDns = new ConfiguradorDnsService();
        InstaladorExtensaoService instaladorExtensao = new InstaladorExtensaoService();
        AutorizadorService autorizador = new AutorizadorService();

        GuiaLarCli cli = new GuiaLarCli(
            detectorDistro,
            detectorNavegador,
            configuradorDns,
            instaladorExtensao,
            autorizador
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

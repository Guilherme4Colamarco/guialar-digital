package br.uniube.pi.guialar.cli;

import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.autorizacao.PlanoAcao;

import java.util.List;

/**
 * Interface de linha de comando do GuiaLar Digital.
 * Executa o fluxo completo de configuração.
 * 
 * Fluxo de segurança:
 * 1. Detecta sistema e navegadores
 * 2. Monta plano de ações
 * 3. Exibe plano e solicita autorização
 * 4. Executa ações autorizadas
 */
public class GuiaLarCli {

    private final DetectorDistroService detectorDistro;
    private final DetectorNavegadorService detectorNavegador;
    private final ConfiguradorDnsService configuradorDns;
    private final InstaladorExtensaoService instaladorExtensao;
    private final AutorizadorService autorizador;

    public GuiaLarCli(
            DetectorDistroService detectorDistro,
            DetectorNavegadorService detectorNavegador,
            ConfiguradorDnsService configuradorDns,
            InstaladorExtensaoService instaladorExtensao,
            AutorizadorService autorizador) {
        this.detectorDistro = detectorDistro;
        this.detectorNavegador = detectorNavegador;
        this.configuradorDns = configuradorDns;
        this.instaladorExtensao = instaladorExtensao;
        this.autorizador = autorizador;
    }

    public void run(String... args) throws Exception {
        imprimirCabecalho();
        
        InfoDistro distro = detectarSistema();
        if (!distro.isSuportada()) {
            System.err.println("\n❌ Distribuição não suportada.");
            System.err.println("   O GuiaLar Digital suporta apenas Debian, Fedora e Arch Linux.");
            System.exit(1);
        }

        List<Navegador> navegadores = detectarNavegadores();

        PlanoAcao plano = criarPlanoAcao(distro, navegadores);

        if (!autorizador.autorizar(plano)) {
            System.out.println("\nOperação cancelada.");
            System.exit(0);
        }

        configurarDns(distro);
        configurarNavegadores(navegadores);

        imprimirRodape();
    }

    private void imprimirCabecalho() {
        System.out.println("╔════════════════════════════════════════════════════════════════╗");
        System.out.println("║              GuiaLar Digital - MVP Linux v0.1.0                ║");
        System.out.println("║    Assistente para DNS seguro e adblockers em Linux            ║");
        System.out.println("╚════════════════════════════════════════════════════════════════╝");
        System.out.println();
    }

    private InfoDistro detectarSistema() {
        System.out.println("🔍 ETAPA 1: Detectando sistema operacional...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        
        InfoDistro distro = detectorDistro.detectar();
        
        System.out.println("Distribuição: " + distro.getNome());
        System.out.println("Versão: " + (distro.getVersao().isEmpty() ? "N/A" : distro.getVersao()));
        System.out.println("Tipo: " + distro.getTipo().getNomeExibicao());
        System.out.println("Gerenciador de rede: " + distro.getGerenciadorRede());
        System.out.println();
        
        return distro;
    }

    private List<Navegador> detectarNavegadores() {
        System.out.println("🌍 ETAPA 2: Detectando navegadores instalados...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        
        List<Navegador> navegadores = detectorNavegador.detectar();
        
        if (navegadores.isEmpty()) {
            System.out.println("⚠ Nenhum navegador detectado.");
        } else {
            System.out.println("Navegadores detectados: " + navegadores.size());
            for (Navegador nav : navegadores) {
                System.out.println("  • " + nav.getNome() + " (" + nav.getTipo().getNomeExibicao() + ")");
            }
        }
        System.out.println();
        
        return navegadores;
    }

    private PlanoAcao criarPlanoAcao(InfoDistro distro, List<Navegador> navegadores) {
        PlanoAcao plano = new PlanoAcao();
        
        ServidorDns servidor = ServidorDns.getPadrao();
        
        plano.adicionarAcao(
            "Configurar DNS do sistema para " + servidor.getNome() + 
            " (IPv4: " + servidor.getPrimario() + " / " + servidor.getSecundario() + 
            ", IPv6: " + servidor.getPrimarioIpv6() + " / " + servidor.getSecundarioIpv6() + ")", 
            true
        );
        
        plano.adicionarAcao("Proteção: " + servidor.getDescricao());
        
        plano.adicionarAcao(
            "Método de configuração: " + distro.getGerenciadorRede() + 
            " (será feito backup antes de modificar)"
        );

        if (!navegadores.isEmpty()) {
            for (Navegador nav : navegadores) {
                String extensao = nav.getTipo() == br.uniube.pi.guialar.dominio.navegador.TipoNavegador.FIREFOX 
                    ? "uBlock Origin" : "uBlock Origin Lite";
                plano.adicionarAcao(
                    "Configurar " + extensao + " no " + nav.getNome()
                );
            }
        }

        plano.adicionarAcao(
            "LIMITAÇÕES: VPN, Docker e DNS-over-HTTPS no navegador podem ignorar o DNS do sistema"
        );

        plano.adicionarAcao("PRIVACIDADE: Nenhum histórico de navegação será coletado");

        return plano;
    }

    private void configurarDns(InfoDistro distro) {
        System.out.println("\n🌐 ETAPA 3: Configurando DNS seguro...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        
        ServidorDns servidor = ServidorDns.getPadrao();
        System.out.println("Servidor DNS: " + servidor.getNome());
        System.out.println("  • IPv4 Primário: " + servidor.getPrimario());
        System.out.println("  • IPv4 Secundário: " + servidor.getSecundario());
        System.out.println("  • IPv6 Primário: " + servidor.getPrimarioIpv6());
        System.out.println("  • IPv6 Secundário: " + servidor.getSecundarioIpv6());
        System.out.println("  • Proteção: " + servidor.getDescricao());
        System.out.println();
        
        ConfiguracaoDns resultado = configuradorDns.configurar(distro);
        
        if (resultado.isAplicado()) {
            System.out.println("✓ " + resultado.getMensagem());
            System.out.println("  Método: " + resultado.getMetodoConfiguracao());
        } else {
            System.err.println("✗ " + resultado.getMensagem());
            System.err.println("  Método tentado: " + resultado.getMetodoConfiguracao());
        }
        System.out.println();
    }

    private void configurarNavegadores(List<Navegador> navegadores) {
        System.out.println("🌍 ETAPA 4: Configurando navegadores...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        
        if (navegadores.isEmpty()) {
            System.out.println("⚠ Nenhum navegador detectado.");
            System.out.println("  Navegadores suportados:");
            System.out.println("  • Base Firefox: Firefox, Firefox ESR, LibreWolf, Waterfox");
            System.out.println("  • Base Chromium: Chromium, Chrome, Brave, Edge, Vivaldi, Opera");
            return;
        }

        for (Navegador navegador : navegadores) {
            System.out.println("→ " + navegador.getNome() + " (" + 
                navegador.getTipo().getNomeExibicao() + ")");
            System.out.println("  Perfil: " + navegador.getCaminhoPerfil());
            
            boolean instalado = instaladorExtensao.instalar(navegador);
            navegador.setExtensaoInstalada(instalado);
            System.out.println();
        }
    }

    private void imprimirRodape() {
        System.out.println("╔════════════════════════════════════════════════════════════════╗");
        System.out.println("║                    Configuração Concluída                      ║");
        System.out.println("╚════════════════════════════════════════════════════════════════╝");
        System.out.println();
        System.out.println("✓ DNS configurado para Cloudflare 1.1.1.1 for Families");
        System.out.println("  • Proteção contra malware");
        System.out.println("  • Bloqueio de conteúdo adulto (18+)");
        System.out.println();
        System.out.println("📋 PRÓXIMOS PASSOS:");
        System.out.println("   1. Reinicie seus navegadores para aplicar as configurações");
        System.out.println("   2. Verifique se o DNS está funcionando:");
        System.out.println("      • Debian/Ubuntu: systemd-resolve --status | grep 'DNS Servers'");
        System.out.println("      • Fedora: nmcli device show | grep DNS");
        System.out.println("      • Arch: resolvectl status");
        System.out.println("   3. Teste bloqueio de malware: http://malware.testcategory.com");
        System.out.println();
        System.out.println("ℹ️  Nenhum dado foi coletado durante a configuração.");
        System.out.println();
    }
}

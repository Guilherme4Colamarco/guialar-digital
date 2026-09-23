package br.uniube.pi.guialar.cli;

import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.aplicacao.plataforma.PlataformaService;
import br.uniube.pi.guialar.dominio.autorizacao.PlanoAcao;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;
import br.uniube.pi.guialar.dominio.sistema.SistemaInfo;

import java.util.List;

/**
 * Interface de linha de comando do GuiaLar Digital.
 *
 * É multiplataforma: todo o comportamento específico de sistema operacional
 * (Linux/Windows) fica em {@link PlataformaService}, escolhido em tempo de
 * execução. O fluxo de segurança (plano de ações + autorização) é preservado.
 */
public class GuiaLarCli {

    private final PlataformaService plataforma;
    private final AutorizadorService autorizador;

    public GuiaLarCli(PlataformaService plataforma, AutorizadorService autorizador) {
        this.plataforma = plataforma;
        this.autorizador = autorizador;
    }

    public void run(String... args) throws Exception {
        imprimirCabecalho();

        SistemaInfo sistema = detectarSistema();
        if (!sistema.isCompativel()) {
            System.err.println("\n❌ Sistema não suportado.");
            System.err.println("   O GuiaLar Digital suporta Windows 10/11 e Linux (Debian, Fedora e Arch).");
            System.exit(1);
        }

        List<Navegador> navegadores = detectarNavegadores();

        PlanoAcao plano = criarPlanoAcao(navegadores);

        boolean admin = plataforma.isAdministrador();
        if (!autorizador.autorizar(plano, admin)) {
            System.out.println("\nOperação cancelada.");
            System.exit(0);
        }

        configurarDns();
        verificarProtecao();
        configurarNavegadores(navegadores);

        imprimirRodape();
    }

    private void imprimirCabecalho() {
        System.out.println("╔════════════════════════════════════════════════════════════════╗");
        System.out.println("║                 GuiaLar Digital - v0.1.0                       ║");
        System.out.println("║    Assistente para DNS seguro e adblockers (Linux + Windows)    ║");
        System.out.println("╚════════════════════════════════════════════════════════════════╝");
        System.out.println();
    }

    private SistemaInfo detectarSistema() {
        System.out.println("🔍 ETAPA 1: Detectando sistema operacional...");
        System.out.println("─────────────────────────────────────────────────────────────────");

        SistemaInfo sistema = plataforma.descreverSistema();

        System.out.println("Sistema: " + sistema.getNomeAmigavel());
        if (sistema.getDetalheTecnico() != null && !sistema.getDetalheTecnico().isEmpty()) {
            System.out.println("Detalhe: " + sistema.getDetalheTecnico());
        }
        System.out.println();

        return sistema;
    }

    private List<Navegador> detectarNavegadores() {
        System.out.println("🌍 ETAPA 2: Detectando navegadores instalados...");
        System.out.println("─────────────────────────────────────────────────────────────────");

        List<Navegador> navegadores = plataforma.detectarNavegadores();

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

    private PlanoAcao criarPlanoAcao(List<Navegador> navegadores) {
        PlanoAcao plano = new PlanoAcao();
        ServidorDns servidor = ServidorDns.getPadrao();

        plano.adicionarAcao(
            "Configurar DNS do sistema para " + servidor.getNome()
                + " (IPv4: " + servidor.getPrimario() + " / " + servidor.getSecundario()
                + ", IPv6: " + servidor.getPrimarioIpv6() + " / " + servidor.getSecundarioIpv6() + ")",
            true);

        plano.adicionarAcao("Proteção: " + servidor.getDescricao());

        for (Navegador nav : navegadores) {
            String extensao = nav.getTipo() == TipoNavegador.FIREFOX
                ? "uBlock Origin" : "uBlock Origin Lite";
            plano.adicionarAcao("Configurar " + extensao + " no " + nav.getNome());
        }

        plano.adicionarAcao(
            "⚠️ AÇÃO OBRIGATÓRIA PÓS-INSTALAÇÃO: configurar DoH dos navegadores para "
                + ServidorDns.DOH_ENDPOINT);
        plano.adicionarAcao("PRIVACIDADE: nenhum histórico de navegação será coletado");

        return plano;
    }

    private void configurarDns() {
        System.out.println("\n🌐 ETAPA 3: Configurando DNS seguro...");
        System.out.println("─────────────────────────────────────────────────────────────────");

        ConfiguracaoDns resultado = plataforma.protegerDns();

        if (resultado.isAplicado()) {
            System.out.println("✓ " + resultado.getMensagem());
            System.out.println("  Método: " + resultado.getMetodoConfiguracao());
        } else {
            System.err.println("✗ " + resultado.getMensagem());
            System.err.println("  Método tentado: " + resultado.getMetodoConfiguracao());
        }
        System.out.println();
    }

    private void verificarProtecao() {
        System.out.println("\n🧪 ETAPA 4: Verificação DNS (Smoke Test)...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();
        plataforma.verificarProtecao();
    }

    private void configurarNavegadores(List<Navegador> navegadores) {
        System.out.println("🌍 ETAPA 5: Configurando navegadores...");
        System.out.println("─────────────────────────────────────────────────────────────────");

        if (navegadores.isEmpty()) {
            System.out.println("⚠ Nenhum navegador detectado.");
            return;
        }

        for (Navegador navegador : navegadores) {
            System.out.println("→ " + navegador.getNome() + " (" + navegador.getTipo().getNomeExibicao() + ")");
            boolean instalado = plataforma.prepararNavegador(navegador);
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
        System.out.println("  • IPv4: 1.1.1.3 / 1.0.0.3");
        System.out.println("  • IPv6: 2606:4700:4700::1113 / 2606:4700:4700::1003");
        System.out.println();
        System.out.println("⚠️  AÇÃO CRÍTICA - DNS-over-HTTPS (DoH)");
        System.out.println("Se seus navegadores usam DoH próprio, configure-os para Families:");
        System.out.println("  " + ServidorDns.DOH_ENDPOINT);
        System.out.println();
        System.out.println("Caso a proteção não tenha sido aplicada: " + plataforma.instrucoesAdmin() + ".");
        System.out.println();
        System.out.println("ℹ️  Nenhum dado foi coletado durante a configuração.");
        System.out.println();
    }
}

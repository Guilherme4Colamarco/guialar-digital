package br.uniube.pi.guialar.cli;

import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.autorizacao.AutorizadorService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.autorizacao.PlanoAcao;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

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
    private final VerificacaoDnsService verificacaoDns;
    private ServidorDns servidorSelecionado = ServidorDns.getPadrao();

    public GuiaLarCli(
            DetectorDistroService detectorDistro,
            DetectorNavegadorService detectorNavegador,
            ConfiguradorDnsService configuradorDns,
            InstaladorExtensaoService instaladorExtensao,
            AutorizadorService autorizador,
            VerificacaoDnsService verificacaoDns) {
        this.detectorDistro = detectorDistro;
        this.detectorNavegador = detectorNavegador;
        this.configuradorDns = configuradorDns;
        this.instaladorExtensao = instaladorExtensao;
        this.autorizador = autorizador;
        this.verificacaoDns = verificacaoDns;
    }

    public void run(String... args) throws Exception {
        servidorSelecionado = resolverDns(args);
        imprimirCabecalho();
        
        InfoDistro distro = detectarSistema();
        if (!distro.isSuportada()) {
            System.err.println("\n❌ Distribuição não suportada.");
            System.err.println("   O GuiaLar Digital suporta Debian, Fedora, Arch Linux e NixOS.");
            System.exit(1);
        }

        List<Navegador> navegadores = detectarNavegadores();

        PlanoAcao plano = criarPlanoAcao(distro, navegadores);

        if (!autorizador.autorizar(plano)) {
            System.out.println("\nOperação cancelada.");
            System.exit(0);
        }

        if (servidorSelecionado.isRequerClienteExterno()) {
            var assist = new br.uniube.pi.guialar.aplicacao.dns.UblockDnsAssistenteService();
            System.out.println(assist.textoAjudaGui());
            System.out.println("Status: " + assist.statusResumo());
            System.out.println("Instale com Profile ID:");
            System.out.println("  " + assist.comandoInstalacaoSugerido(null, null));
            System.out.println("Ou use a GUI: java -jar guialar-digital.jar --gui");
            configurarNavegadores(navegadores);
            return;
        }

        configurarDns(distro);
        verificarDns();
        configurarNavegadores(navegadores);

        imprimirRodape();
    }

    private static ServidorDns resolverDns(String... args) {
        for (String a : args) {
            if (a != null && a.startsWith("--dns=")) {
                return ServidorDns.deArgumento(a.substring("--dns=".length()));
            }
        }
        return ServidorDns.getPadrao();
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
        
        ServidorDns servidor = servidorSelecionado;
        
        plano.adicionarAcao(
            "Configurar DNS do sistema (todos os apps) para " + servidor.getNome() + 
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
            "⚠️ AÇÃO OBRIGATÓRIA PÓS-INSTALAÇÃO: Configurar DoH dos navegadores"
        );
        
        plano.adicionarAcao(
            "   Firefox/Chrome devem usar DoH: " + servidor.getDohEndpoint()
        );
        
        plano.adicionarAcao(
            "   Caso contrário, o navegador IGNORA o DNS do sistema e o filtro NÃO funciona!"
        );

        plano.adicionarAcao(
            "LIMITAÇÕES: VPN e Docker podem ignorar o DNS do sistema"
        );

        plano.adicionarAcao("PRIVACIDADE: Nenhum histórico de navegação será coletado");

        return plano;
    }

    private void configurarDns(InfoDistro distro) {
        System.out.println("\n🌐 ETAPA 3: Configurando DNS seguro...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        
        ServidorDns servidor = servidorSelecionado;
        System.out.println("Servidor DNS (todos os apps neste computador): " + servidor.getNome());
        System.out.println("  • IPv4 Primário: " + servidor.getPrimario());
        System.out.println("  • IPv4 Secundário: " + servidor.getSecundario());
        System.out.println("  • IPv6 Primário: " + servidor.getPrimarioIpv6());
        System.out.println("  • IPv6 Secundário: " + servidor.getSecundarioIpv6());
        System.out.println("  • Proteção: " + servidor.getDescricao());
        System.out.println("  • DoH: " + servidor.getDohEndpoint());
        System.out.println();
        
        ConfiguracaoDns resultado = configuradorDns.configurar(distro, servidor);

        if (resultado.isAguardandoUsuario()) {
            System.out.println("⏳ Aguardando você aplicar: " + resultado.getMensagem());
            System.out.println("--- Trecho para copiar ---");
            System.out.println(resultado.getTextoParaCopiar());
        } else if (resultado.isAplicado()) {
            System.out.println("✓ " + resultado.getMensagem());
            System.out.println("  Método: " + resultado.getMetodoConfiguracao());
        } else {
            System.err.println("○ " + resultado.getMensagem());
            System.err.println("  Método tentado: " + resultado.getMetodoConfiguracao());
        }
        System.out.println();
    }

    private void verificarDns() {
        System.out.println("\n🧪 ETAPA 4: Verificação DNS (Smoke Test)...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();
        
        List<ResultadoVerificacao> resultados = verificacaoDns.verificar(servidorSelecionado);
        verificacaoDns.exibirResumo(resultados, servidorSelecionado);
    }

    private void configurarNavegadores(List<Navegador> navegadores) {
        System.out.println("🌍 ETAPA 5: Configurando navegadores...");
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
        ServidorDns s = servidorSelecionado;
        System.out.println("Revise o resumo da verificação acima antes de considerar a proteção ativa.");
        System.out.println("  • Alvo: " + s.getNome());
        System.out.println("  • IPv4: " + s.getPrimario() + " / " + s.getSecundario());
        System.out.println("  • IPv6: " + s.getPrimarioIpv6() + " / " + s.getSecundarioIpv6());
        System.out.println("  • " + s.getDescricao());
        System.out.println();
        System.out.println("⚠️  AÇÃO CRÍTICA NECESSÁRIA - DNS-over-HTTPS (DoH)");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();
        System.out.println("Se seus navegadores usam DNS Seguro/DoH, eles IGNORAM o DNS do");
        System.out.println("sistema e o filtro NÃO FUNCIONA!");
        System.out.println();
        System.out.println("Configure DoH dos navegadores para:");
        System.out.println("  " + s.getDohEndpoint());
        System.out.println();
        System.out.println("Flags: --dns=families | --dns=adguard (alias: --dns=ublock)");
        System.out.println();
        System.out.println("ℹ️  Nenhum dado foi coletado durante a configuração.");
        System.out.println();
    }
}

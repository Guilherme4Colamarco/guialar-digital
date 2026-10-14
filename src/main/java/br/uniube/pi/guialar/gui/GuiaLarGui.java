package br.uniube.pi.guialar.gui;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsUrlOpener;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorSistemaService;
import br.uniube.pi.guialar.aplicacao.diagnostico.DiagnosticoAmbienteService;
import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxDnsManifestStore;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsLinuxService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsWindowsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.verificacao.LinuxProtecaoStatusService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;
import br.uniube.pi.guialar.dominio.diagnostico.DiagnosticoAmbiente;
import br.uniube.pi.guialar.dominio.diagnostico.StatusDns;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import javax.swing.BorderFactory;
import javax.swing.Box;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JOptionPane;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextArea;
import javax.swing.SwingUtilities;
import javax.swing.SwingWorker;
import javax.swing.UIManager;
import javax.swing.border.EmptyBorder;

import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Component;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.GridLayout;
import java.awt.Toolkit;
import java.awt.datatransfer.StringSelection;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Scanner;
import java.util.prefs.Preferences;

/**
 * Interface gráfica (Swing) do GuiaLar Digital.
 *
 * Linux: fluxo completo (DNS + smoke test + extensões).
 * Windows: diagnóstico sem admin + tentativa de DNS com UAC + guias de navegador.
 * Falhas de DNS em laboratório mostram "não aplicado" sem crash.
 */
public class GuiaLarGui {

    private static final Color COR_TEXTO_CLARO = Color.WHITE;
    private static final String PREF_TEMA = "tema";
    private static final float FONTE_CORPO = 15f;
    private static final float FONTE_TITULO_CARTAO = 16f;
    private static final float FONTE_TITULO_APP = 28f;
    private static final float FONTE_SUBTITULO = 16f;
    private static final float FONTE_STATUS = 22f;
    private static final float FONTE_RODAPE = 14f;
    private static final float FONTE_BOTAO_PRIMARIO = 15f;
    private static final float FONTE_MONO = 14f;

    private static final String[] CHAVES_UI_TEMA_ESCURO = {
        "Panel.background", "OptionPane.background", "ScrollPane.background", "Viewport.background",
        "Label.foreground", "OptionPane.messageForeground", "Button.foreground", "TextArea.foreground",
        "Button.background", "Button.select", "Button.focus"
    };

    private static Tema temaInicialLinhaComando;
    private static boolean temaSessaoSeguirSistema;
    private static Map<String, Object> uiDefaultsAntesDoEscuro;

    private Tema tema;
    private final DetectorSistemaService detectorSistema;
    private final DetectorDistroService detectorDistro;
    private final DetectorNavegadorService detectorNavegador;
    private final ConfiguradorDnsService configuradorDns;
    private final ConfiguradorDnsLinuxService configuradorDnsLinux;
    private final ConfiguradorDnsWindowsService configuradorDnsWindows;
    private final InstaladorExtensaoService instaladorExtensao;
    private final VerificacaoDnsService verificacaoDns;
    private final DiagnosticoAmbienteService diagnosticoService;
    private final WindowsUrlOpener urlOpener;

    private JFrame frame;
    private JTextArea areaLog;
    private JButton botaoExecutar;
    private JButton botaoDiagnostico;
    private JButton botaoGuias;
    private JButton botaoDesfazer;
    private JButton botaoCopiarConfig;
    private JButton botaoSair;
    private String textoCopiarNixOs;
    private JLabel statusAmigavel;
    private JLabel detalheAmigavel;
    private JPanel detalhesTecnicos;

    private TipoSistema tipoSistema;
    private InfoDistro distro;
    private DiagnosticoAmbiente diagnostico;
    private List<Navegador> navegadores = new ArrayList<>();

    public GuiaLarGui() {
        this.detectorSistema = new DetectorSistemaService();
        this.detectorDistro = new DetectorDistroService();
        this.detectorNavegador = new DetectorNavegadorService();
        this.configuradorDnsLinux = new ConfiguradorDnsLinuxService();
        this.configuradorDns = new ConfiguradorDnsService(configuradorDnsLinux);
        this.configuradorDnsWindows = new ConfiguradorDnsWindowsService();
        this.instaladorExtensao = new InstaladorExtensaoService();
        this.verificacaoDns = new VerificacaoDnsService();
        this.diagnosticoService = new DiagnosticoAmbienteService();
        this.urlOpener = new WindowsUrlOpener();
    }

    public static void iniciar() {
        iniciar(null);
    }

    /** @param temaForcado tema da linha de comando ({@code null} = preferência salva ou sistema) */
    public static void iniciar(Tema temaForcado) {
        iniciar(temaForcado, false);
    }

    public static void iniciar(Tema temaForcado, boolean seguirSistemaNestaSessao) {
        temaInicialLinhaComando = temaForcado;
        temaSessaoSeguirSistema = seguirSistemaNestaSessao;
        SwingUtilities.invokeLater(() -> {
            try {
                new GuiaLarGui().iniciarConstrucao();
            } catch (Exception ex) {
                ex.printStackTrace();
                JOptionPane.showMessageDialog(null,
                    "Falha ao abrir a interface (o sistema não foi alterado):\n" + ex.getMessage(),
                    "GuiaLar Digital", JOptionPane.ERROR_MESSAGE);
            }
        });
    }

    private void iniciarConstrucao() {
        if (temaInicialLinhaComando != null) {
            tema = temaInicialLinhaComando;
            construirComTema();
            return;
        }
        new SwingWorker<Tema, Void>() {
            @Override
            protected Tema doInBackground() {
                if (temaSessaoSeguirSistema) {
                    return DeteccaoTemaSistema.detectar();
                }
                return lerPreferenciaTemaForaDaEdt();
            }

            @Override
            protected void done() {
                try {
                    tema = get();
                } catch (Exception e) {
                    tema = Tema.CLARO;
                }
                construirComTema();
            }
        }.execute();
    }

    private Tema lerPreferenciaTemaForaDaEdt() {
        try {
            String valor = Preferences.userNodeForPackage(GuiaLarGui.class).get(PREF_TEMA, "auto");
            if ("auto".equalsIgnoreCase(valor)) {
                return DeteccaoTemaSistema.detectar();
            }
            return Optional.ofNullable(Tema.de(valor)).orElse(DeteccaoTemaSistema.detectar());
        } catch (Exception e) {
            return DeteccaoTemaSistema.detectar();
        }
    }

    private void construirComTema() {
        aplicarLookAndFeel();

        tipoSistema = detectorSistema.detectar();
        if (tipoSistema.isWindows()) {
            diagnostico = diagnosticoService.diagnosticar();
            navegadores = deduplicar(diagnostico.getNavegadores());
        } else {
            distro = detectorDistro.detectar();
            navegadores = deduplicar(detectorNavegador.detectar());
            diagnostico = diagnosticoService.diagnosticar();
            textoCopiarNixOs = new LinuxProtecaoStatusService().carregarSnippetNixosPersistido();
        }

        frame = new JFrame("GuiaLar Digital - DNS seguro e adblockers");
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.setMinimumSize(new Dimension(900, 680));

        frame.setContentPane(criarRaiz());
        frame.pack();
        frame.setLocationRelativeTo(null);
        frame.setVisible(true);

        atualizarBotoesManifesto();
        log("GuiaLar Digital iniciado (" + tipoSistema.getNome() + ").");
        if (tipoSistema.isWindows()) {
            log("Modo Windows: diagnóstico não exige admin. DNS só sob UAC.");
            log("Status DNS: " + diagnostico.getStatusDns().getRotuloPt());
        } else {
            log("Revise o plano de ações e clique em \"Autorizar e executar\".");
        }
    }

    private JPanel criarRaiz() {
        JPanel raiz = new JPanel(new BorderLayout());
        raiz.setBackground(tema.fundo());
        raiz.add(criarCabecalho(), BorderLayout.NORTH);
        raiz.add(criarCorpo(), BorderLayout.CENTER);
        raiz.add(criarRodape(), BorderLayout.SOUTH);
        return raiz;
    }

    private void salvarPreferenciaTema(Tema escolhido) {
        try {
            Preferences.userNodeForPackage(GuiaLarGui.class)
                .put(PREF_TEMA, escolhido.isEscuro() ? "escuro" : "claro");
        } catch (Exception ignored) {
        }
    }

    private void garantirDefaultsSistemaCapturados() throws Exception {
        if (uiDefaultsAntesDoEscuro != null) {
            return;
        }
        UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
        uiDefaultsAntesDoEscuro = new HashMap<>();
        for (String chave : CHAVES_UI_TEMA_ESCURO) {
            uiDefaultsAntesDoEscuro.put(chave, UIManager.get(chave));
        }
    }

    private void aplicarLookAndFeel() {
        try {
            if (tema.isEscuro()) {
                garantirDefaultsSistemaCapturados();
                UIManager.setLookAndFeel(javax.swing.plaf.metal.MetalLookAndFeel.class.getName());
                aplicarCoresDialogosEBotoes();
            } else {
                UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
                if (uiDefaultsAntesDoEscuro != null) {
                    for (var e : uiDefaultsAntesDoEscuro.entrySet()) {
                        UIManager.put(e.getKey(), e.getValue());
                    }
                }
            }
        } catch (Exception ignored) {
        }
    }

    private void aplicarCoresDialogosEBotoes() {
        Color botao = new Color(0x2C, 0x36, 0x40);
        for (String chave : new String[] {"Panel.background", "OptionPane.background",
                "ScrollPane.background", "Viewport.background"}) {
            UIManager.put(chave, tema.cartao());
        }
        for (String chave : new String[] {"Label.foreground", "OptionPane.messageForeground",
                "Button.foreground", "TextArea.foreground"}) {
            UIManager.put(chave, tema.texto());
        }
        UIManager.put("Button.background", botao);
        UIManager.put("Button.select", tema.borda());
        UIManager.put("Button.focus", tema.primaria());
    }

    private void alternarTema() {
        String logAtual = areaLog == null ? "" : areaLog.getText();
        boolean detalhesVisiveis = detalhesTecnicos != null && detalhesTecnicos.isVisible();
        tema = tema.alternar();
        salvarPreferenciaTema(tema);
        aplicarLookAndFeel();
        frame.setContentPane(criarRaiz());
        if (detalhesTecnicos != null) {
            detalhesTecnicos.setVisible(detalhesVisiveis);
        }
        if (areaLog != null) {
            areaLog.setText(logAtual);
            areaLog.setCaretPosition(areaLog.getDocument().getLength());
        }
        SwingUtilities.updateComponentTreeUI(frame);
        frame.revalidate();
        frame.repaint();
    }

    private JPanel criarCabecalho() {
        JPanel header = new JPanel();
        header.setLayout(new BoxLayout(header, BoxLayout.Y_AXIS));
        header.setBackground(tema.cabecalho());
        header.setBorder(new EmptyBorder(18, 24, 18, 24));

        JLabel titulo = new JLabel("GuiaLar");
        titulo.setForeground(COR_TEXTO_CLARO);
        titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, FONTE_TITULO_APP));
        titulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        JLabel subtitulo = new JLabel(
            "Proteja a navegação da sua família");
        subtitulo.setForeground(tema.cabecalhoSub());
        subtitulo.setFont(subtitulo.getFont().deriveFont(Font.PLAIN, FONTE_SUBTITULO));
        subtitulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        header.add(titulo);
        header.add(Box.createVerticalStrut(4));
        header.add(subtitulo);

        JButton botaoTema = new JButton(tema.isEscuro() ? "Tema claro" : "Tema escuro");
        botaoTema.addActionListener(e -> alternarTema());
        JPanel topo = new JPanel(new BorderLayout());
        topo.setBackground(tema.cabecalho());
        JPanel lateral = new JPanel(new FlowLayout(FlowLayout.RIGHT, 16, 16));
        lateral.setBackground(tema.cabecalho());
        lateral.add(botaoTema);
        topo.add(header, BorderLayout.CENTER);
        topo.add(lateral, BorderLayout.EAST);
        return topo;
    }

    private JPanel criarCorpo() {
        JPanel corpo = new JPanel();
        corpo.setLayout(new BoxLayout(corpo, BoxLayout.Y_AXIS));
        corpo.setBackground(tema.fundo());
        corpo.setBorder(new EmptyBorder(16, 20, 8, 20));

        JPanel status = criarCartao("Como está a proteção");
        statusAmigavel = linha("");
        statusAmigavel.setFont(statusAmigavel.getFont().deriveFont(Font.BOLD, FONTE_STATUS));
        detalheAmigavel = linha("");
        detalheAmigavel.setFont(detalheAmigavel.getFont().deriveFont(Font.PLAIN, FONTE_CORPO));
        status.add(statusAmigavel);
        status.add(detalheAmigavel);
        status.setMaximumSize(new Dimension(Integer.MAX_VALUE, 130));
        corpo.add(status);
        corpo.add(Box.createVerticalStrut(14));

        JPanel tarefas = new JPanel(new GridLayout(1, 3, 12, 0));
        tarefas.setBackground(tema.fundo());
        tarefas.setAlignmentX(Component.LEFT_ALIGNMENT);
        tarefas.add(criarCartaoTarefa("1. Tentar ativar proteção", "Filtra sites perigosos e inadequados. Pode pedir autorização do responsável pelo computador."));
        tarefas.add(criarCartaoTarefa("2. Proteger navegadores", "Veja instruções simples para bloquear anúncios no Edge, Chrome e outros navegadores."));
        tarefas.add(criarCartaoTarefa("3. Verificar novamente", "Confira se a proteção já está ativa nesta rede."));
        corpo.add(tarefas);
        corpo.add(Box.createVerticalStrut(14));
        corpo.add(criarResumoEncontrado());
        corpo.add(Box.createVerticalStrut(8));
        JButton detalhes = new JButton("Ver detalhes técnicos");
        detalhes.setAlignmentX(Component.LEFT_ALIGNMENT);
        detalhes.addActionListener(e -> alternarDetalhesTecnicos());
        corpo.add(detalhes);
        detalhesTecnicos = new JPanel();
        detalhesTecnicos.setLayout(new BoxLayout(detalhesTecnicos, BoxLayout.Y_AXIS));
        detalhesTecnicos.setBackground(tema.fundo());
        detalhesTecnicos.setAlignmentX(Component.LEFT_ALIGNMENT);
        detalhesTecnicos.setVisible(false);
        JPanel tecnicos = new JPanel(new GridLayout(1, 3, 10, 0));
        tecnicos.setBackground(tema.fundo());
        tecnicos.setAlignmentX(Component.LEFT_ALIGNMENT);
        tecnicos.add(criarCartaoSistema());
        tecnicos.add(criarCartaoDns());
        tecnicos.add(criarCartaoNavegadores());
        detalhesTecnicos.add(tecnicos);
        detalhesTecnicos.add(Box.createVerticalStrut(10));
        detalhesTecnicos.add(criarCartaoPlano());
        detalhesTecnicos.add(Box.createVerticalStrut(10));
        detalhesTecnicos.add(criarCartaoLog());
        corpo.add(detalhesTecnicos);
        corpo.add(Box.createVerticalStrut(8));
        JLabel ajuda = linha("Você pode usar o GuiaLar sem mudar nada. Em computadores da faculdade ou do trabalho, peça ajuda ao suporte local.");
        ajuda.setForeground(tema.secundario());
        corpo.add(ajuda);
        atualizarResumoAmigavel();
        return corpo;
    }

    private JPanel criarCartaoTarefa(String titulo, String descricao) {
        JPanel p = criarCartao(titulo);
        JLabel d = linha("<html><body style='width: 205px'>" + descricao + "</body></html>");
        d.setFont(d.getFont().deriveFont(Font.PLAIN, FONTE_CORPO));
        p.add(d);
        return p;
    }

    private JPanel criarResumoEncontrado() {
        JPanel p = criarCartao("O que encontramos");
        String nomes = navegadores.isEmpty() ? "Nenhum navegador foi encontrado automaticamente." : "Navegadores encontrados: "
            + String.join(", ", navegadores.stream().map(Navegador::getNome).toList());
        p.add(linha(nomes));
        p.add(linha(estadoAmigavelDns()));
        return p;
    }

    private String estadoAmigavelDns() {
        if (diagnostico == null) return "Não foi possível verificar a rede ainda.";
        return switch (diagnostico.getStatusDns()) {
            case APLICADO -> "Proteção encontrada nesta rede.";
            case LEITURA_BLOQUEADA -> "A rede bloqueou a consulta. Nenhuma alteração foi feita.";
            case PARCIAL -> "Parte da proteção foi encontrada; confirme os detalhes antes de continuar.";
            case NAO_APLICADO -> "A proteção da rede ainda não está ativa pelo GuiaLar.";
            case AGUARDANDO_APLICACAO -> "Falta você colar a configuração no NixOS e rodar o rebuild.";
            case DESCONHECIDO -> "A proteção ainda não pôde ser verificada.";
        };
    }

    private void alternarDetalhesTecnicos() {
        detalhesTecnicos.setVisible(!detalhesTecnicos.isVisible());
        frame.revalidate();
        frame.pack();
    }

    private void atualizarResumoAmigavel() {
        if (statusAmigavel == null || diagnostico == null) return;
        StatusDns estado = diagnostico.getStatusDns();
        if (estado == StatusDns.APLICADO) {
            statusAmigavel.setText("Protegido");
            detalheAmigavel.setText("A verificação confirmou que sites perigosos estão bloqueados nesta rede.");
            statusAmigavel.setForeground(tema.ok());
        } else if (estado == StatusDns.LEITURA_BLOQUEADA) {
            statusAmigavel.setText("A proteção ainda não foi confirmada");
            detalheAmigavel.setText("A rede não permitiu consultar essa configuração. Isso é comum em computadores da faculdade ou do trabalho. Nada foi alterado.");
            statusAmigavel.setForeground(tema.atencao());
        } else if (estado == StatusDns.AGUARDANDO_APLICACAO) {
            statusAmigavel.setText("Aguardando você aplicar");
            detalheAmigavel.setText("Copie o trecho para o configuration.nix, rode sudo nixos-rebuild switch e clique em Verificar de novo.");
            statusAmigavel.setForeground(tema.atencao());
        } else {
            statusAmigavel.setText("A proteção da rede não está ativa pelo GuiaLar");
            detalheAmigavel.setText("Você ainda pode seguir os passos para proteger seus navegadores. Nenhuma alteração será feita sem sua autorização.");
            statusAmigavel.setForeground(tema.atencao());
        }
    }

    private JPanel criarCartao(String tituloCartao) {
        JPanel cartao = new JPanel();
        cartao.setLayout(new BoxLayout(cartao, BoxLayout.Y_AXIS));
        cartao.setBackground(tema.cartao());
        cartao.setBorder(BorderFactory.createCompoundBorder(
            BorderFactory.createLineBorder(tema.borda()),
            new EmptyBorder(12, 14, 14, 14)));
        cartao.setAlignmentX(Component.LEFT_ALIGNMENT);

        JLabel titulo = new JLabel(tituloCartao);
        titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, FONTE_TITULO_CARTAO));
        titulo.setForeground(tema.titulo());
        titulo.setAlignmentX(Component.LEFT_ALIGNMENT);
        cartao.add(titulo);
        cartao.add(Box.createVerticalStrut(8));
        return cartao;
    }

    private JLabel linha(String texto) {
        JLabel l = new JLabel(texto);
        l.setFont(l.getFont().deriveFont(Font.PLAIN, FONTE_CORPO));
        l.setForeground(tema.texto());
        l.setAlignmentX(Component.LEFT_ALIGNMENT);
        return l;
    }

    private JPanel criarCartaoSistema() {
        JPanel cartao = criarCartao("Sistema detectado");
        if (tipoSistema.isWindows() && diagnostico != null) {
            cartao.add(linha("SO: " + diagnostico.getNomeSistema()));
            cartao.add(linha("Versão: " + diagnostico.getVersaoSistema()));
            cartao.add(linha("Arquitetura: " + diagnostico.getArquitetura()));
            cartao.add(linha("Elevado (admin): " + (diagnostico.isProcessoElevado() ? "sim" : "não")));
            cartao.add(linha("Pode pedir UAC: " + (diagnostico.isPodeElevar() ? "sim" : "não")));
            cartao.add(Box.createVerticalStrut(6));
            JLabel status = linha("Status: Windows suportado (smoke test)");
            status.setForeground(tema.ok());
            status.setFont(status.getFont().deriveFont(Font.BOLD, FONTE_CORPO));
            cartao.add(status);
        } else {
            boolean suportada = distro != null && distro.isSuportada();
            cartao.add(linha("Distribuição: " + (distro == null ? "?" : distro.getNome())));
            cartao.add(linha("Versão: " + (distro == null || distro.getVersao() == null
                || distro.getVersao().isEmpty() ? "N/A" : distro.getVersao())));
            cartao.add(linha("Família: " + (distro == null ? "?" : distro.getTipo().getNomeExibicao())));
            cartao.add(linha("Gerenciador de rede: "
                + (distro == null ? "?" : distro.getGerenciadorRede())));
            cartao.add(Box.createVerticalStrut(6));
            JLabel status = linha(suportada ? "Status: suportada (MVP Linux)" : "Status: NÃO suportada");
            status.setForeground(suportada ? tema.ok() : tema.erro());
            status.setFont(status.getFont().deriveFont(Font.BOLD, FONTE_CORPO));
            cartao.add(status);
        }
        return cartao;
    }

    private JPanel criarCartaoDns() {
        ServidorDns s = ServidorDns.getPadrao();
        JPanel cartao = criarCartao("DNS Cloudflare Families");
        cartao.add(linha("IPv4: " + s.getPrimario() + " / " + s.getSecundario()));
        cartao.add(linha("IPv6: " + s.getPrimarioIpv6()));
        cartao.add(linha("        " + s.getSecundarioIpv6()));
        cartao.add(Box.createVerticalStrut(6));
        cartao.add(linha("Proteção: malware + adulto (18+)"));
        if (diagnostico != null) {
            cartao.add(Box.createVerticalStrut(6));
            JLabel st = linha("Estado GuiaLar: " + diagnostico.getStatusDns().getRotuloPt());
            Color cor = diagnostico.getStatusDns() == StatusDns.APLICADO
                ? tema.ok() : tema.atencao();
            st.setForeground(cor);
            st.setFont(st.getFont().deriveFont(Font.BOLD, FONTE_CORPO));
            cartao.add(st);
        }
        return cartao;
    }

    private JPanel criarCartaoNavegadores() {
        JPanel cartao = criarCartao("Navegadores detectados");
        if (navegadores.isEmpty()) {
            cartao.add(linha("Nenhum navegador detectado."));
        } else {
            for (Navegador nav : navegadores) {
                String dica = nav.getNome().toLowerCase().contains("brave")
                    ? "Shields + DNS seguro"
                    : (nav.getTipo() == TipoNavegador.FIREFOX
                        ? "uBlock Origin" : "uBlock Origin Lite");
                cartao.add(linha("• " + nav.getNome()));
                cartao.add(linha("    → " + dica));
            }
        }
        return cartao;
    }

    private JPanel criarCartaoPlano() {
        JPanel cartao = criarCartao("Plano / diagnóstico (nada muda sem autorização)");
        JTextArea plano = new JTextArea(montarTextoPlano());
        plano.setEditable(false);
        plano.setLineWrap(true);
        plano.setWrapStyleWord(true);
        plano.setFont(new Font("Dialog", Font.PLAIN, 12).deriveFont(FONTE_CORPO));
        plano.setBackground(tema.planoFundo());
        plano.setForeground(tema.texto());
        plano.setBorder(new EmptyBorder(6, 6, 6, 6));
        JScrollPane sp = new JScrollPane(plano);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 160));
        sp.setMaximumSize(new Dimension(Integer.MAX_VALUE, 180));
        cartao.add(sp);
        return cartao;
    }

    private JPanel criarCartaoLog() {
        JPanel cartao = criarCartao("Execução");
        areaLog = new JTextArea();
        areaLog.setEditable(false);
        areaLog.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12).deriveFont(FONTE_MONO));
        areaLog.setBackground(tema.logFundo());
        areaLog.setForeground(tema.logTexto());
        areaLog.setBorder(new EmptyBorder(8, 8, 8, 8));
        JScrollPane sp = new JScrollPane(areaLog);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 220));
        cartao.add(sp);
        return cartao;
    }

    private JPanel criarRodape() {
        JPanel rodape = new JPanel(new BorderLayout());
        rodape.setBackground(tema.fundo());
        rodape.setBorder(new EmptyBorder(6, 20, 16, 20));

        String avisoTxt = tipoSistema.isWindows()
            ? "Diagnóstico não precisa de admin. Aplicar DNS pode pedir UAC (ou falhar em laboratório)."
            : "Só a troca de DNS pede senha de administrador (pkexec). O resto roda como usuário normal.";
        JLabel aviso = new JLabel(tipoSistema.isWindows()
            ? "Você decide antes de qualquer mudança. Em PCs gerenciados, peça ajuda ao suporte."
            : avisoTxt);
        aviso.setFont(aviso.getFont().deriveFont(Font.PLAIN, FONTE_RODAPE));
        aviso.setForeground(tema.secundario());

        JPanel botoes = new JPanel();
        botoes.setBackground(tema.fundo());

        botaoSair = new JButton("Sair");
        botaoSair.addActionListener(e -> frame.dispose());

        botaoDiagnostico = new JButton("Atualizar diagnóstico");
        botaoDiagnostico.addActionListener(e -> onDiagnostico());

        botaoGuias = new JButton("Abrir guias");
        botaoGuias.addActionListener(e -> onAbrirGuias());

        botaoDesfazer = new JButton("Desfazer proteção");
        botaoDesfazer.setVisible(manifestoDesfazerVisivel());
        botaoDesfazer.addActionListener(e -> onDesfazer());

        botaoCopiarConfig = new JButton("Copiar configuração");
        botaoCopiarConfig.setVisible(textoCopiarNixOs != null && !textoCopiarNixOs.isBlank());
        botaoCopiarConfig.setFont(botaoCopiarConfig.getFont().deriveFont(Font.BOLD, 14f));
        botaoCopiarConfig.addActionListener(e -> copiarConfiguracaoNixOs());

        botaoExecutar = new JButton(tipoSistema.isWindows()
            ? "Tentar ativar proteção" : "Ativar proteção");
        botaoExecutar.setBackground(tema.primaria());
        botaoExecutar.setForeground(COR_TEXTO_CLARO);
        botaoExecutar.setFont(botaoExecutar.getFont().deriveFont(Font.BOLD, FONTE_BOTAO_PRIMARIO));
        botaoExecutar.setOpaque(true);
        botaoExecutar.setBorderPainted(false);
        botaoExecutar.setFocusPainted(false);
        botaoExecutar.addActionListener(e -> onExecutar());

        botoes.add(botaoSair);
        botaoDiagnostico.setText("Verificar novamente");
        botoes.add(botaoDiagnostico);
        botaoGuias.setText("Proteger navegadores");
        botoes.add(botaoGuias);
        botoes.add(botaoCopiarConfig);
        botoes.add(botaoDesfazer);
        botoes.add(botaoExecutar);

        rodape.add(aviso, BorderLayout.WEST);
        rodape.add(botoes, BorderLayout.EAST);
        return rodape;
    }

    private String montarTextoPlano() {
        if (tipoSistema.isWindows() && diagnostico != null) {
            StringBuilder sb = new StringBuilder();
            sb.append(diagnostico.formatarRelatorio()).append('\n');
            sb.append("Ações opcionais:\n");
            sb.append("1. Tentar aplicar DNS (UAC) — pode resultar em \"não aplicado\"\n");
            sb.append("2. Abrir guias de uBlock / Shields / DoH no navegador\n");
            sb.append("3. Desfazer DNS usando o manifesto em %LOCALAPPDATA%\\GuiaLar\\\n");
            return sb.toString();
        }
        StringBuilder sb = new StringBuilder();
        ServidorDns s = ServidorDns.getPadrao();
        int i = 1;
        sb.append(i++).append(". Configurar DNS do sistema para ").append(s.getNome())
          .append(" (IPv4: ").append(s.getPrimario()).append(" / ").append(s.getSecundario())
          .append(")\n");
        sb.append(i++).append(". Método: ").append(distro.getGerenciadorRede())
          .append(" (backup antes de modificar)\n");
        for (Navegador nav : navegadores) {
            String extensao = nav.getTipo() == TipoNavegador.FIREFOX
                ? "uBlock Origin" : "uBlock Origin Lite";
            sb.append(i++).append(". Configurar ").append(extensao)
              .append(" no ").append(nav.getNome()).append("\n");
        }
        sb.append(i++).append(". Verificar o DNS com smoke test\n");
        sb.append(i++).append(". Pós-instalação: DoH → ").append(ServidorDns.DOH_ENDPOINT).append("\n");
        sb.append(i).append(". Privacidade: nenhum histórico de navegação é coletado.");
        return sb.toString();
    }

    private void onDiagnostico() {
        botaoDiagnostico.setEnabled(false);
        new SwingWorker<DiagnosticoAmbiente, Void>() {
            @Override
            protected DiagnosticoAmbiente doInBackground() {
                if (tipoSistema != null && !tipoSistema.isWindows()) {
                    return diagnosticoService.diagnosticarLinuxComVerificacao(tipoSistema);
                }
                return diagnosticoService.diagnosticar();
            }

            @Override
            protected void done() {
                try {
                    diagnostico = get();
                    navegadores = deduplicar(diagnostico.getNavegadores());
                    if (tipoSistema != null && !tipoSistema.isWindows()) {
                        textoCopiarNixOs = new LinuxProtecaoStatusService().carregarSnippetNixosPersistido();
                    }
                    atualizarResumoAmigavel();
                    atualizarBotoesManifesto();
                    log("\n--- Verificação atualizada ---\n" + diagnostico.formatarRelatorio());
                    if (diagnostico.getStatusDns() != StatusDns.APLICADO) {
                        log("Filtro de sistema: NÃO afirmado como ativo (status: "
                            + diagnostico.getStatusDns().getRotuloPt() + ").");
                    }
                } catch (Exception ex) {
                    log("Falha no diagnóstico (sistema inalterado): " + ex.getMessage());
                } finally {
                    botaoDiagnostico.setEnabled(true);
                }
            }
        }.execute();
    }

    private void onAbrirGuias() {
        if (navegadores.isEmpty()) {
            JOptionPane.showMessageDialog(frame,
                "Nenhum navegador detectado para abrir guias.",
                "Guias", JOptionPane.INFORMATION_MESSAGE);
            return;
        }
        for (Navegador nav : navegadores) {
            log("Guia " + nav.getNome() + ":");
            for (String linha : WindowsUrlOpener.guiasPara(nav.getNome())) {
                log("  " + linha);
            }
            String url = WindowsUrlOpener.urlLojaPadrao(nav.getNome());
            boolean ok = urlOpener.abrir(url);
            log("  Abrir: " + (ok ? "ok" : "falhou") + " → " + url);
        }
    }

    private void onDesfazer() {
        String extra = tipoSistema.isWindows()
            ? "Pode solicitar UAC."
            : "Pode solicitar pkexec (senha de administrador).";
        int opcao = JOptionPane.showConfirmDialog(frame,
            "Desfazer alterações de DNS feitas pelo GuiaLar?\n"
                + extra + " Sem manifesto, nada será alterado.",
            "Desfazer proteção", JOptionPane.YES_NO_OPTION);
        if (opcao != JOptionPane.YES_OPTION) {
            return;
        }
        botaoDesfazer.setEnabled(false);
        new SwingWorker<ConfiguracaoDns, Void>() {
            @Override
            protected ConfiguracaoDns doInBackground() {
                if (tipoSistema.isWindows()) {
                    return configuradorDnsWindows.desfazer();
                }
                return configuradorDnsLinux.desfazer();
            }

            @Override
            protected void done() {
                try {
                    ConfiguracaoDns r = get();
                    log(r.isAplicado() ? "✓ " + r.getMensagem() : "○ " + r.getMensagem());
                    if (!tipoSistema.isWindows() && r.getTextoParaCopiar() != null) {
                        textoCopiarNixOs = r.getTextoParaCopiar();
                    }
                    atualizarBotoesManifesto();
                } catch (Exception ex) {
                    log("Erro ao desfazer (sistema pode estar inalterado): " + ex.getMessage());
                } finally {
                    botaoDesfazer.setEnabled(true);
                    onDiagnostico();
                }
            }
        }.execute();
    }

    private void onExecutar() {
        if (tipoSistema.isWindows()) {
            onExecutarWindows();
            return;
        }
        onExecutarLinux();
    }

    private void onExecutarWindows() {
        StringBuilder msg = new StringBuilder();
        msg.append("Será solicitada elevação UAC apenas para alterar o DNS.\n\n");
        msg.append("Alvo: Cloudflare Families (1.1.1.3 / 1.0.0.3).\n");
        msg.append("Um manifesto será salvo em %LOCALAPPDATA%\\GuiaLar\\ para Desfazer.\n\n");
        msg.append("Em PC de laboratório sem admin, o resultado esperado é\n");
        msg.append("\"DNS não aplicado\" — sem alterar o sistema.\n\n");
        msg.append("Continuar?");

        int opcao = JOptionPane.showConfirmDialog(frame, msg.toString(),
            "Aplicar DNS (Windows)", JOptionPane.YES_NO_OPTION, JOptionPane.WARNING_MESSAGE);
        if (opcao != JOptionPane.YES_OPTION) {
            log("Operação cancelada pelo usuário.");
            return;
        }

        botaoExecutar.setEnabled(false);
        botaoExecutar.setText("Aplicando...");
        new SwingWorker<ConfiguracaoDns, Void>() {
            @Override
            protected ConfiguracaoDns doInBackground() {
                return configuradorDnsWindows.configurar();
            }

            @Override
            protected void done() {
                try {
                    ConfiguracaoDns r = get();
                    if (r.isAplicado()) {
                        log("✓ " + r.getMensagem());
                        log("Ainda configure DoH nos navegadores: " + ServidorDns.DOH_ENDPOINT);
                    } else {
                        log("○ " + r.getMensagem());
                        log("Status: " + StatusDns.NAO_APLICADO.getRotuloPt()
                            + " — filtro de sistema NÃO está ativo via GuiaLar.");
                        log("O que ainda funciona: diagnóstico + guias de navegador.");
                        JOptionPane.showMessageDialog(frame,
                            "DNS não aplicado.\n\n" + r.getMensagem()
                                + "\n\nO sistema não foi alterado.\n"
                                + "Você ainda pode usar o diagnóstico e os guias.",
                            "DNS não aplicado", JOptionPane.WARNING_MESSAGE);
                    }
                } catch (Exception ex) {
                    log("Falha ao aplicar DNS (sistema inalterado): " + ex.getMessage());
                    JOptionPane.showMessageDialog(frame,
                        "Falha ao aplicar DNS. O sistema não foi alterado.\n" + ex.getMessage(),
                        "Erro", JOptionPane.ERROR_MESSAGE);
                } finally {
                    botaoExecutar.setText("Tentar ativar proteção");
                    botaoExecutar.setEnabled(true);
                    onDiagnostico();
                }
            }
        }.execute();
    }

    private void onExecutarLinux() {
        if (distro == null || !distro.isSuportada()) {
            JOptionPane.showMessageDialog(frame,
                "Distribuição não suportada.\nO GuiaLar Digital suporta Debian, Ubuntu, Fedora, Arch Linux e NixOS.",
                "Não suportado", JOptionPane.ERROR_MESSAGE);
            return;
        }

        boolean root = isRoot();
        StringBuilder msg = new StringBuilder();
        msg.append("As seguintes ações modificam o seu sistema:\n\n");
        msg.append(montarTextoPlano()).append("\n\n");
        msg.append("A troca de DNS pedirá autorização do administrador (pkexec).\n");
        msg.append("O manifesto para Desfazer ficará em ~/.guialar/\n\n");
        msg.append("Deseja autorizar e continuar?");

        int opcao = JOptionPane.showConfirmDialog(frame, msg.toString(),
            "Autorização necessária", JOptionPane.YES_NO_OPTION,
            root ? JOptionPane.QUESTION_MESSAGE : JOptionPane.WARNING_MESSAGE);

        if (opcao != JOptionPane.YES_OPTION) {
            log("\nOperação cancelada pelo usuário.");
            return;
        }

        executarPlanoLinux();
    }

    private void executarPlanoLinux() {
        botaoExecutar.setEnabled(false);
        botaoExecutar.setText("Executando...");
        log("\n✓ Autorização concedida. Executando ações...\n");

        new SwingWorker<ConfiguracaoDns, Void>() {
            @Override
            protected ConfiguracaoDns doInBackground() {
                java.io.PrintStream original = System.out;
                java.io.PrintStream originalErr = System.err;
                java.io.PrintStream ponte = new java.io.PrintStream(
                    new AreaLogOutputStream(), true, java.nio.charset.StandardCharsets.UTF_8);
                System.setOut(ponte);
                System.setErr(ponte);
                ConfiguracaoDns resultado = null;
                try {
                    System.out.println("[ETAPA 1/3] Configurando DNS seguro (pkexec só nesta etapa)...");
                    resultado = configuradorDns.configurar(distro);
                    if (resultado.isAguardandoUsuario()) {
                        System.out.println("  AGUARDANDO: " + resultado.getMensagem());
                    } else if (resultado.isAplicado()) {
                        System.out.println("  OK: " + resultado.getMensagem());
                    } else {
                        System.out.println("  FALHOU: " + resultado.getMensagem());
                    }

                    if (!resultado.isAguardandoUsuario() && resultado.isAplicado()) {
                        System.out.println("\n[ETAPA 2/3] Verificação DNS já executada após aplicar.");
                    } else if (resultado.isAguardandoUsuario()) {
                        System.out.println("\n[ETAPA 2/3] Aguardando você aplicar no NixOS — verificação depois.");
                    } else {
                        System.out.println("\n[ETAPA 2/3] DNS não aplicado — verificação omitida.");
                    }

                    System.out.println("[ETAPA 3/3] Configurando navegadores...");
                    if (navegadores.isEmpty()) {
                        System.out.println("  Nenhum navegador para configurar.");
                    }
                    for (Navegador nav : navegadores) {
                        System.out.println("  → " + nav.getNome());
                        boolean ok = instaladorExtensao.instalar(nav);
                        nav.setExtensaoInstalada(ok);
                    }

                    System.out.println("\n──────────────────────────────────────────────");
                    System.out.println("AÇÃO CRÍTICA: configure o DoH dos navegadores para:");
                    System.out.println("  " + ServidorDns.DOH_ENDPOINT);
                } catch (Exception ex) {
                    System.out.println("Erro durante a execução: " + ex.getMessage());
                } finally {
                    System.setOut(original);
                    System.setErr(originalErr);
                }
                return resultado;
            }

            @Override
            protected void done() {
                try {
                    ConfiguracaoDns resultado = get();
                    if (resultado != null) {
                        if (resultado.isAguardandoUsuario()) {
                            textoCopiarNixOs = resultado.getTextoParaCopiar();
                            diagnostico = diagnosticoService.diagnosticar();
                            atualizarResumoAmigavel();
                            JOptionPane.showMessageDialog(frame,
                                "Copie a configuração com o botão \"Copiar configuração\",\n"
                                    + "aplique no NixOS e depois clique em \"Verificar de novo\".",
                                "Aguardando você aplicar", JOptionPane.INFORMATION_MESSAGE);
                        } else if (!resultado.isAplicado()) {
                            JOptionPane.showMessageDialog(frame, resultado.getMensagem(),
                                "Não foi possível aplicar", JOptionPane.WARNING_MESSAGE);
                        } else {
                            diagnostico = diagnosticoService.diagnosticarLinuxComVerificacao(tipoSistema);
                            atualizarResumoAmigavel();
                        }
                    }
                } catch (Exception ignored) {
                }
                atualizarBotoesManifesto();
                botaoExecutar.setText("Ativar proteção");
                botaoExecutar.setEnabled(true);
                log("\nProcesso concluído.");
            }
        }.execute();
    }

    private boolean manifestoDesfazerVisivel() {
        if (tipoSistema == null) {
            return false;
        }
        if (tipoSistema.isWindows()) {
            return new br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsManifestStore().existe();
        }
        return new LinuxDnsManifestStore().existe();
    }

    private void atualizarBotoesManifesto() {
        if (botaoDesfazer != null) {
            botaoDesfazer.setVisible(manifestoDesfazerVisivel());
        }
        if (botaoCopiarConfig != null) {
            botaoCopiarConfig.setVisible(textoCopiarNixOs != null && !textoCopiarNixOs.isBlank());
        }
    }

    private void copiarConfiguracaoNixOs() {
        if (textoCopiarNixOs == null || textoCopiarNixOs.isBlank()) {
            return;
        }
        Toolkit.getDefaultToolkit().getSystemClipboard()
            .setContents(new StringSelection(textoCopiarNixOs), null);
        log("Configuração copiada para a área de transferência.");
        JOptionPane.showMessageDialog(frame,
            "Configuração copiada. Cole no configuration.nix e rode:\nsudo nixos-rebuild switch",
            "Copiar configuração", JOptionPane.INFORMATION_MESSAGE);
    }

    private boolean isRoot() {
        if ("root".equals(System.getProperty("user.name"))) {
            return true;
        }
        try {
            Process p = new ProcessBuilder("id", "-u").redirectErrorStream(true).start();
            try (Scanner sc = new Scanner(p.getInputStream())) {
                if (sc.hasNextInt()) {
                    return sc.nextInt() == 0;
                }
            }
        } catch (Exception ignored) {
        }
        return false;
    }

    private List<Navegador> deduplicar(List<Navegador> lista) {
        Map<String, Navegador> unicos = new LinkedHashMap<>();
        for (Navegador nav : lista) {
            unicos.putIfAbsent(nav.getNome() + "|" + nav.getTipo(), nav);
        }
        return new ArrayList<>(unicos.values());
    }

    private void log(String texto) {
        if (areaLog == null) {
            return;
        }
        SwingUtilities.invokeLater(() -> {
            areaLog.append(texto + "\n");
            areaLog.setCaretPosition(areaLog.getDocument().getLength());
        });
    }

    private class AreaLogOutputStream extends java.io.OutputStream {
        private final StringBuilder buffer = new StringBuilder();

        @Override
        public synchronized void write(int b) {
            if (b == '\n') {
                emitir();
            } else if (b != '\r') {
                buffer.append((char) b);
            }
        }

        @Override
        public synchronized void write(byte[] b, int off, int len) {
            for (int i = off; i < off + len; i++) {
                write(b[i]);
            }
        }

        private void emitir() {
            final String linha = buffer.toString();
            buffer.setLength(0);
            SwingUtilities.invokeLater(() -> {
                areaLog.append(linha + "\n");
                areaLog.setCaretPosition(areaLog.getDocument().getLength());
            });
        }
    }
}


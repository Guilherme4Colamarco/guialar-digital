package br.uniube.pi.guialar.gui;

import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
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
import java.awt.Font;
import java.awt.GridLayout;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Scanner;

/**
 * Interface gráfica (Swing) do GuiaLar Digital.
 *
 * Reutiliza os mesmos serviços de domínio/aplicação da CLI, expondo o fluxo
 * completo de forma visual:
 * 1. Detecta a distribuição Linux e os navegadores instalados.
 * 2. Exibe o plano de ações transparente antes de qualquer alteração.
 * 3. Solicita autorização explícita do usuário (botão) e verifica privilégios.
 * 4. Executa a configuração de DNS, o smoke test e a instalação de adblockers,
 *    transmitindo toda a saída para um painel de log em tempo real.
 *
 * Implementada com Swing (parte do JDK) para não introduzir dependências novas
 * ao build Ant do projeto.
 */
public class GuiaLarGui {

    private static final Color COR_FUNDO = new Color(0xF4, 0xF6, 0xF8);
    private static final Color COR_HEADER = new Color(0x0B, 0x5F, 0x63);
    private static final Color COR_PRIMARIA = new Color(0x0E, 0x7A, 0x80);
    private static final Color COR_TEXTO_CLARO = Color.WHITE;

    private final DetectorDistroService detectorDistro;
    private final DetectorNavegadorService detectorNavegador;
    private final ConfiguradorDnsService configuradorDns;
    private final InstaladorExtensaoService instaladorExtensao;
    private final VerificacaoDnsService verificacaoDns;

    private JFrame frame;
    private JTextArea areaLog;
    private JButton botaoExecutar;
    private JButton botaoSair;

    private InfoDistro distro;
    private List<Navegador> navegadores = new ArrayList<>();

    public GuiaLarGui() {
        this.detectorDistro = new DetectorDistroService();
        this.detectorNavegador = new DetectorNavegadorService();
        this.configuradorDns = new ConfiguradorDnsService();
        this.instaladorExtensao = new InstaladorExtensaoService();
        this.verificacaoDns = new VerificacaoDnsService();
    }

    /** Ponto de entrada da GUI. */
    public static void iniciar() {
        SwingUtilities.invokeLater(() -> new GuiaLarGui().construir());
    }

    private void construir() {
        aplicarLookAndFeel();

        distro = detectorDistro.detectar();
        navegadores = deduplicar(detectorNavegador.detectar());

        frame = new JFrame("GuiaLar Digital - DNS seguro e adblockers");
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.setMinimumSize(new Dimension(860, 720));

        JPanel raiz = new JPanel(new BorderLayout());
        raiz.setBackground(COR_FUNDO);

        raiz.add(criarCabecalho(), BorderLayout.NORTH);
        raiz.add(criarCorpo(), BorderLayout.CENTER);
        raiz.add(criarRodape(), BorderLayout.SOUTH);

        frame.setContentPane(raiz);
        frame.pack();
        frame.setLocationRelativeTo(null);
        frame.setVisible(true);

        log("GuiaLar Digital iniciado. Revise o plano de ações e clique em \"Autorizar e executar\".");
    }

    private void aplicarLookAndFeel() {
        try {
            UIManager.setLookAndFeel(UIManager.getSystemLookAndFeelClassName());
        } catch (Exception ignored) {
            // Usa o look and feel padrão caso o do sistema não esteja disponível.
        }
    }

    private JPanel criarCabecalho() {
        JPanel header = new JPanel();
        header.setLayout(new BoxLayout(header, BoxLayout.Y_AXIS));
        header.setBackground(COR_HEADER);
        header.setBorder(new EmptyBorder(18, 24, 18, 24));

        JLabel titulo = new JLabel("GuiaLar Digital");
        titulo.setForeground(COR_TEXTO_CLARO);
        titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, 26f));
        titulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        JLabel subtitulo = new JLabel(
            "Assistente para configuração de DNS seguro (Cloudflare Families) e adblockers");
        subtitulo.setForeground(new Color(0xD6, 0xEE, 0xEF));
        subtitulo.setFont(subtitulo.getFont().deriveFont(Font.PLAIN, 14f));
        subtitulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        header.add(titulo);
        header.add(Box.createVerticalStrut(4));
        header.add(subtitulo);
        return header;
    }

    private JPanel criarCorpo() {
        JPanel corpo = new JPanel();
        corpo.setLayout(new BoxLayout(corpo, BoxLayout.Y_AXIS));
        corpo.setBackground(COR_FUNDO);
        corpo.setBorder(new EmptyBorder(16, 20, 8, 20));

        JPanel cartoes = new JPanel(new GridLayout(1, 3, 12, 0));
        cartoes.setBackground(COR_FUNDO);
        cartoes.setAlignmentX(Component.LEFT_ALIGNMENT);
        cartoes.add(criarCartaoSistema());
        cartoes.add(criarCartaoDns());
        cartoes.add(criarCartaoNavegadores());
        corpo.add(cartoes);

        corpo.add(Box.createVerticalStrut(14));
        corpo.add(criarCartaoPlano());

        corpo.add(Box.createVerticalStrut(14));
        corpo.add(criarCartaoLog());

        return corpo;
    }

    private JPanel criarCartao(String tituloCartao) {
        JPanel cartao = new JPanel();
        cartao.setLayout(new BoxLayout(cartao, BoxLayout.Y_AXIS));
        cartao.setBackground(Color.WHITE);
        cartao.setBorder(BorderFactory.createCompoundBorder(
            BorderFactory.createLineBorder(new Color(0xE0, 0xE4, 0xE8)),
            new EmptyBorder(12, 14, 14, 14)));
        cartao.setAlignmentX(Component.LEFT_ALIGNMENT);

        JLabel titulo = new JLabel(tituloCartao);
        titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, 13f));
        titulo.setForeground(COR_HEADER);
        titulo.setAlignmentX(Component.LEFT_ALIGNMENT);
        cartao.add(titulo);
        cartao.add(Box.createVerticalStrut(8));
        return cartao;
    }

    private JLabel linha(String texto) {
        JLabel l = new JLabel(texto);
        l.setFont(l.getFont().deriveFont(Font.PLAIN, 12.5f));
        l.setAlignmentX(Component.LEFT_ALIGNMENT);
        return l;
    }

    private JPanel criarCartaoSistema() {
        JPanel cartao = criarCartao("Sistema detectado");
        boolean suportada = distro.isSuportada();
        cartao.add(linha("Distribuição: " + distro.getNome()));
        cartao.add(linha("Versão: " + (distro.getVersao() == null || distro.getVersao().isEmpty()
            ? "N/A" : distro.getVersao())));
        cartao.add(linha("Família: " + distro.getTipo().getNomeExibicao()));
        cartao.add(linha("Gerenciador de rede: " + distro.getGerenciadorRede()));
        cartao.add(Box.createVerticalStrut(6));
        JLabel status = linha(suportada ? "Status: suportada (MVP Linux)" : "Status: NÃO suportada");
        status.setForeground(suportada ? new Color(0x1B, 0x7A, 0x2E) : new Color(0xB3, 0x26, 0x1A));
        status.setFont(status.getFont().deriveFont(Font.BOLD, 12.5f));
        cartao.add(status);
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
        cartao.add(Box.createVerticalStrut(6));
        cartao.add(linha("DoH: family.cloudflare-dns.com"));
        return cartao;
    }

    private JPanel criarCartaoNavegadores() {
        JPanel cartao = criarCartao("Navegadores detectados");
        if (navegadores.isEmpty()) {
            cartao.add(linha("Nenhum navegador detectado."));
        } else {
            for (Navegador nav : navegadores) {
                String extensao = nav.getTipo() == TipoNavegador.FIREFOX
                    ? "uBlock Origin" : "uBlock Origin Lite";
                cartao.add(linha("• " + nav.getNome() + " (" + nav.getTipo().getNomeExibicao() + ")"));
                cartao.add(linha("    → " + extensao));
            }
        }
        return cartao;
    }

    private JPanel criarCartaoPlano() {
        JPanel cartao = criarCartao("Plano de ações (nada é executado sem sua autorização)");
        JTextArea plano = new JTextArea(montarTextoPlano());
        plano.setEditable(false);
        plano.setLineWrap(true);
        plano.setWrapStyleWord(true);
        plano.setFont(new Font("Dialog", Font.PLAIN, 12));
        plano.setBackground(new Color(0xF9, 0xFB, 0xFC));
        plano.setBorder(new EmptyBorder(6, 6, 6, 6));
        JScrollPane sp = new JScrollPane(plano);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 150));
        sp.setMaximumSize(new Dimension(Integer.MAX_VALUE, 160));
        cartao.add(sp);
        return cartao;
    }

    private JPanel criarCartaoLog() {
        JPanel cartao = criarCartao("Execução");
        areaLog = new JTextArea();
        areaLog.setEditable(false);
        areaLog.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        areaLog.setBackground(new Color(0x1E, 0x24, 0x2B));
        areaLog.setForeground(new Color(0xE6, 0xE6, 0xE6));
        areaLog.setBorder(new EmptyBorder(8, 8, 8, 8));
        JScrollPane sp = new JScrollPane(areaLog);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 220));
        cartao.add(sp);
        return cartao;
    }

    private JPanel criarRodape() {
        JPanel rodape = new JPanel(new BorderLayout());
        rodape.setBackground(COR_FUNDO);
        rodape.setBorder(new EmptyBorder(6, 20, 16, 20));

        JLabel aviso = new JLabel(
            "A configuração de DNS exige privilégios de administrador (sudo/root).");
        aviso.setFont(aviso.getFont().deriveFont(Font.PLAIN, 11.5f));
        aviso.setForeground(new Color(0x5A, 0x63, 0x6B));

        JPanel botoes = new JPanel();
        botoes.setBackground(COR_FUNDO);

        botaoSair = new JButton("Sair");
        botaoSair.addActionListener(e -> frame.dispose());

        botaoExecutar = new JButton("Autorizar e executar");
        botaoExecutar.setBackground(COR_PRIMARIA);
        botaoExecutar.setForeground(COR_TEXTO_CLARO);
        botaoExecutar.setFont(botaoExecutar.getFont().deriveFont(Font.BOLD, 13f));
        botaoExecutar.setOpaque(true);
        botaoExecutar.setBorderPainted(false);
        botaoExecutar.setFocusPainted(false);
        botaoExecutar.addActionListener(e -> onExecutar());

        botoes.add(botaoSair);
        botoes.add(botaoExecutar);

        rodape.add(aviso, BorderLayout.WEST);
        rodape.add(botoes, BorderLayout.EAST);
        return rodape;
    }

    private String montarTextoPlano() {
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
        sb.append(i++).append(". Verificar o DNS com smoke test (malware/nudity bloqueados, example.com permitido)\n");
        sb.append(i++).append(". Pós-instalação: configurar DoH dos navegadores para ")
          .append(ServidorDns.DOH_ENDPOINT).append("\n");
        sb.append(i).append(". Privacidade: nenhum histórico de navegação é coletado.");
        return sb.toString();
    }

    private void onExecutar() {
        if (!distro.isSuportada()) {
            JOptionPane.showMessageDialog(frame,
                "Distribuição não suportada.\nO GuiaLar Digital suporta Debian, Fedora e Arch Linux.",
                "Não suportado", JOptionPane.ERROR_MESSAGE);
            return;
        }

        boolean root = isRoot();
        StringBuilder msg = new StringBuilder();
        msg.append("As seguintes ações modificam o seu sistema:\n\n");
        msg.append(montarTextoPlano()).append("\n\n");
        if (!root) {
            msg.append("Aviso: o programa NÃO está em modo administrador. A troca de DNS\n");
            msg.append("provavelmente falhará. Reabra com: sudo java -jar guialar-digital.jar --gui\n\n");
        }
        msg.append("Deseja autorizar e continuar?");

        int opcao = JOptionPane.showConfirmDialog(frame, msg.toString(),
            "Autorização necessária", JOptionPane.YES_NO_OPTION,
            root ? JOptionPane.QUESTION_MESSAGE : JOptionPane.WARNING_MESSAGE);

        if (opcao != JOptionPane.YES_OPTION) {
            log("\nOperação cancelada pelo usuário.");
            return;
        }

        executarPlano();
    }

    private void executarPlano() {
        botaoExecutar.setEnabled(false);
        botaoExecutar.setText("Executando...");
        log("\n✓ Autorização concedida. Executando ações...\n");

        new SwingWorker<Void, Void>() {
            @Override
            protected Void doInBackground() {
                java.io.PrintStream original = System.out;
                java.io.PrintStream originalErr = System.err;
                java.io.PrintStream ponte = new java.io.PrintStream(
                    new AreaLogOutputStream(), true, java.nio.charset.StandardCharsets.UTF_8);
                System.setOut(ponte);
                System.setErr(ponte);
                try {
                    System.out.println("[ETAPA 1/3] Configurando DNS seguro...");
                    ConfiguracaoDns resultado = configuradorDns.configurar(distro);
                    if (resultado.isAplicado()) {
                        System.out.println("  OK: " + resultado.getMensagem());
                    } else {
                        System.out.println("  FALHOU: " + resultado.getMensagem());
                    }

                    System.out.println("\n[ETAPA 2/3] Verificação DNS (smoke test)...");
                    List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
                    verificacaoDns.exibirResumo(resultados);

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
                    System.out.println("Caso contrário, o navegador ignora o DNS do sistema.");
                } catch (Exception ex) {
                    System.out.println("Erro durante a execução: " + ex.getMessage());
                } finally {
                    System.setOut(original);
                    System.setErr(originalErr);
                }
                return null;
            }

            @Override
            protected void done() {
                botaoExecutar.setText("Autorizar e executar");
                botaoExecutar.setEnabled(true);
                log("\nProcesso concluído.");
            }
        }.execute();
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
            // Sem privilégios administrativos.
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

    /** Redireciona a saída padrão dos serviços para o painel de log da GUI. */
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

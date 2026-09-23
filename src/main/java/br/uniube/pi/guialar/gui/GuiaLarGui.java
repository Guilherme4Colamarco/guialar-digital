package br.uniube.pi.guialar.gui;

import br.uniube.pi.guialar.aplicacao.plataforma.PlataformaFactory;
import br.uniube.pi.guialar.aplicacao.plataforma.PlataformaService;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.SistemaInfo;
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
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Interface gráfica (Swing) do GuiaLar Digital em "Modo Simples".
 *
 * O objetivo desta tela é permitir que pessoas leigas usem o programa sem
 * precisar entender termos técnicos (DNS, DoH, adblocker etc.). Toda a
 * comunicação principal é feita em linguagem de benefício ("bloquear sites
 * perigosos") e os termos técnicos ficam escondidos em um painel opcional
 * "Detalhes técnicos".
 *
 * As diretrizes de linguagem estão documentadas no agente de UX em
 * {@code .cursor/rules/interface-amigavel.mdc}.
 *
 * Reutiliza os mesmos serviços de domínio/aplicação da CLI. Implementada com
 * Swing (parte do JDK) para não introduzir dependências novas ao build Ant.
 */
public class GuiaLarGui {

    private static final Color COR_FUNDO = new Color(0xF4, 0xF6, 0xF8);
    private static final Color COR_HEADER = new Color(0x0B, 0x5F, 0x63);
    private static final Color COR_PRIMARIA = new Color(0x1B, 0x7A, 0x2E);
    private static final Color COR_TEXTO_CLARO = Color.WHITE;
    private static final Color COR_OK = new Color(0x1B, 0x7A, 0x2E);
    private static final Color COR_ATENCAO = new Color(0xB3, 0x5A, 0x00);

    private final PlataformaService plataforma;

    private JFrame frame;
    private JTextArea areaProgresso;
    private JTextArea areaTecnica;
    private JPanel painelTecnico;
    private JButton botaoProteger;
    private JButton botaoDetalhes;

    private SistemaInfo sistema;
    private List<Navegador> navegadores = new ArrayList<>();

    public GuiaLarGui() {
        this.plataforma = PlataformaFactory.criar();
    }

    /** Ponto de entrada da GUI. */
    public static void iniciar() {
        SwingUtilities.invokeLater(() -> new GuiaLarGui().construir());
    }

    private void construir() {
        aplicarLookAndFeel();

        sistema = plataforma.descreverSistema();
        navegadores = deduplicar(plataforma.detectarNavegadores());

        frame = new JFrame("GuiaLar Digital");
        frame.setDefaultCloseOperation(JFrame.EXIT_ON_CLOSE);
        frame.setMinimumSize(new Dimension(820, 720));

        JPanel raiz = new JPanel(new BorderLayout());
        raiz.setBackground(COR_FUNDO);
        raiz.add(criarCabecalho(), BorderLayout.NORTH);
        raiz.add(criarCorpo(), BorderLayout.CENTER);
        raiz.add(criarRodape(), BorderLayout.SOUTH);

        frame.setContentPane(raiz);
        frame.pack();
        frame.setLocationRelativeTo(null);
        frame.setVisible(true);

        progresso("Tudo pronto. Quando quiser, clique em \"Proteger meu computador\".");
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
        header.setBorder(new EmptyBorder(20, 26, 20, 26));

        JLabel titulo = new JLabel("GuiaLar Digital");
        titulo.setForeground(COR_TEXTO_CLARO);
        titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, 28f));
        titulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        JLabel subtitulo = new JLabel("Deixe a internet da sua família mais segura com um clique.");
        subtitulo.setForeground(new Color(0xD6, 0xEE, 0xEF));
        subtitulo.setFont(subtitulo.getFont().deriveFont(Font.PLAIN, 15f));
        subtitulo.setAlignmentX(Component.LEFT_ALIGNMENT);

        header.add(titulo);
        header.add(Box.createVerticalStrut(6));
        header.add(subtitulo);
        return header;
    }

    private JPanel criarCorpo() {
        JPanel corpo = new JPanel();
        corpo.setLayout(new BoxLayout(corpo, BoxLayout.Y_AXIS));
        corpo.setBackground(COR_FUNDO);
        corpo.setBorder(new EmptyBorder(18, 22, 10, 22));

        corpo.add(criarCartaoOQueFaz());
        corpo.add(Box.createVerticalStrut(14));
        corpo.add(criarCartaoProgresso());
        corpo.add(Box.createVerticalStrut(12));
        corpo.add(criarPainelTecnico());
        return corpo;
    }

    private JPanel criarCartao(String tituloCartao) {
        JPanel cartao = new JPanel();
        cartao.setLayout(new BoxLayout(cartao, BoxLayout.Y_AXIS));
        cartao.setBackground(Color.WHITE);
        cartao.setBorder(BorderFactory.createCompoundBorder(
            BorderFactory.createLineBorder(new Color(0xE0, 0xE4, 0xE8)),
            new EmptyBorder(14, 16, 16, 16)));
        cartao.setAlignmentX(Component.LEFT_ALIGNMENT);

        if (tituloCartao != null) {
            JLabel titulo = new JLabel(tituloCartao);
            titulo.setFont(titulo.getFont().deriveFont(Font.BOLD, 16f));
            titulo.setForeground(COR_HEADER);
            titulo.setAlignmentX(Component.LEFT_ALIGNMENT);
            cartao.add(titulo);
            cartao.add(Box.createVerticalStrut(10));
        }
        return cartao;
    }

    private JLabel item(String texto) {
        JLabel l = new JLabel("<html><body style='width:640px'>" + texto + "</body></html>");
        l.setFont(l.getFont().deriveFont(Font.PLAIN, 14f));
        l.setAlignmentX(Component.LEFT_ALIGNMENT);
        l.setBorder(new EmptyBorder(3, 0, 3, 0));
        return l;
    }

    private JPanel criarCartaoOQueFaz() {
        JPanel cartao = criarCartao("O que este programa faz por você");
        cartao.add(item("✓ &nbsp;Bloqueia <b>sites perigosos</b> (vírus e golpes)."));
        cartao.add(item("✓ &nbsp;Bloqueia <b>conteúdo impróprio para menores</b> (+18)."));
        String navTexto = navegadores.isEmpty()
            ? "✓ &nbsp;Prepara um <b>bloqueador de anúncios</b> nos seus navegadores."
            : "✓ &nbsp;Instala um <b>bloqueador de anúncios</b> em: " + nomesNavegadores() + ".";
        cartao.add(item(navTexto));
        cartao.add(Box.createVerticalStrut(8));

        boolean compativel = sistema.isCompativel();
        JLabel status = item(compativel
            ? "• &nbsp;Seu computador (" + sistema.getNomeAmigavel() + ") é <b>compatível</b> — pode continuar."
            : "• &nbsp;Seu computador ainda <b>não é compatível</b> com o GuiaLar.");
        status.setForeground(compativel ? COR_OK : COR_ATENCAO);
        cartao.add(status);

        cartao.add(Box.createVerticalStrut(6));
        JLabel tranquilizar = item("É seguro: nada é feito sem a sua permissão, "
            + "guardamos suas configurações para poder desfazer e "
            + "<b>nenhuma informação sua é coletada</b>.");
        tranquilizar.setForeground(new Color(0x5A, 0x63, 0x6B));
        cartao.add(tranquilizar);
        return cartao;
    }

    private JPanel criarCartaoProgresso() {
        JPanel cartao = criarCartao("Como está a proteção");
        areaProgresso = new JTextArea();
        areaProgresso.setEditable(false);
        areaProgresso.setLineWrap(true);
        areaProgresso.setWrapStyleWord(true);
        areaProgresso.setFont(new Font("Dialog", Font.PLAIN, 15));
        areaProgresso.setBackground(new Color(0xF3, 0xF9, 0xF4));
        areaProgresso.setForeground(new Color(0x21, 0x2B, 0x24));
        areaProgresso.setBorder(new EmptyBorder(10, 10, 10, 10));
        JScrollPane sp = new JScrollPane(areaProgresso);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 210));
        cartao.add(sp);
        return cartao;
    }

    private JPanel criarPainelTecnico() {
        painelTecnico = criarCartao("Detalhes técnicos (para quem entende do assunto)");
        areaTecnica = new JTextArea();
        areaTecnica.setEditable(false);
        areaTecnica.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        areaTecnica.setBackground(new Color(0x1E, 0x24, 0x2B));
        areaTecnica.setForeground(new Color(0xE6, 0xE6, 0xE6));
        areaTecnica.setBorder(new EmptyBorder(8, 8, 8, 8));
        JScrollPane sp = new JScrollPane(areaTecnica);
        sp.setAlignmentX(Component.LEFT_ALIGNMENT);
        sp.setPreferredSize(new Dimension(10, 200));
        painelTecnico.add(sp);
        painelTecnico.setVisible(false);
        return painelTecnico;
    }

    private JPanel criarRodape() {
        JPanel rodape = new JPanel(new BorderLayout());
        rodape.setBackground(COR_FUNDO);
        rodape.setBorder(new EmptyBorder(4, 22, 18, 22));

        botaoDetalhes = new JButton("Mostrar detalhes técnicos");
        botaoDetalhes.setFocusPainted(false);
        botaoDetalhes.addActionListener(e -> alternarDetalhes());

        JPanel esquerda = new JPanel();
        esquerda.setBackground(COR_FUNDO);
        esquerda.add(botaoDetalhes);

        JButton botaoSair = new JButton("Sair");
        botaoSair.addActionListener(e -> frame.dispose());

        botaoProteger = new JButton("Proteger meu computador");
        botaoProteger.setBackground(COR_PRIMARIA);
        botaoProteger.setForeground(COR_TEXTO_CLARO);
        botaoProteger.setFont(botaoProteger.getFont().deriveFont(Font.BOLD, 15f));
        botaoProteger.setOpaque(true);
        botaoProteger.setBorderPainted(false);
        botaoProteger.setFocusPainted(false);
        botaoProteger.setPreferredSize(new Dimension(260, 46));
        botaoProteger.addActionListener(e -> onProteger());

        JPanel direita = new JPanel();
        direita.setBackground(COR_FUNDO);
        direita.add(botaoSair);
        direita.add(botaoProteger);

        rodape.add(esquerda, BorderLayout.WEST);
        rodape.add(direita, BorderLayout.EAST);
        return rodape;
    }

    private void alternarDetalhes() {
        boolean mostrar = !painelTecnico.isVisible();
        painelTecnico.setVisible(mostrar);
        botaoDetalhes.setText(mostrar ? "Ocultar detalhes técnicos" : "Mostrar detalhes técnicos");
        frame.revalidate();
        frame.repaint();
    }

    private void onProteger() {
        if (!sistema.isCompativel()) {
            JOptionPane.showMessageDialog(frame,
                "Este computador ainda não é compatível com o GuiaLar Digital.\n"
                    + "Ele funciona no Windows 10/11 e nos sistemas Linux mais comuns "
                    + "(Debian, Ubuntu, Fedora e Arch).",
                "Ainda não dá para continuar", JOptionPane.INFORMATION_MESSAGE);
            return;
        }

        boolean admin = plataforma.isAdministrador();
        StringBuilder msg = new StringBuilder();
        msg.append("Vamos deixar seu computador mais seguro. Isto vai:\n\n");
        msg.append("   • Bloquear sites perigosos (vírus e golpes)\n");
        msg.append("   • Bloquear conteúdo impróprio para menores (+18)\n");
        if (!navegadores.isEmpty()) {
            msg.append("   • Preparar um bloqueador de anúncios em ").append(nomesNavegadores()).append("\n");
        }
        msg.append("\nVocê pode desfazer isso depois, quando quiser.\n");
        if (!admin) {
            msg.append("\nObservação: pode ser que o programa peça permissão de administrador\n");
            msg.append("para concluir a proteção.\n");
        }
        msg.append("\nPodemos começar?");

        int opcao = JOptionPane.showConfirmDialog(frame, msg.toString(),
            "Proteger meu computador", JOptionPane.YES_NO_OPTION, JOptionPane.QUESTION_MESSAGE);
        if (opcao != JOptionPane.YES_OPTION) {
            progresso("Sem problemas! Nada foi alterado. Você pode fazer isso mais tarde.");
            return;
        }
        executar();
    }

    private void executar() {
        botaoProteger.setEnabled(false);
        botaoProteger.setText("Protegendo…");
        areaProgresso.setText("");
        progresso("Começando a proteger seu computador…");

        new SwingWorker<Void, String>() {
            private boolean dnsOk;
            private int testesOk;
            private int testesTotal;

            @Override
            protected Void doInBackground() {
                java.io.PrintStream original = System.out;
                java.io.PrintStream originalErr = System.err;
                java.io.PrintStream ponte = new java.io.PrintStream(
                    new AreaTecnicaOutputStream(), true, java.nio.charset.StandardCharsets.UTF_8);
                System.setOut(ponte);
                System.setErr(ponte);
                try {
                    publish("Ativando a proteção contra sites perigosos e conteúdo +18…");
                    ConfiguracaoDns resultado = plataforma.protegerDns();
                    dnsOk = resultado.isAplicado();
                    publish(dnsOk
                        ? "✓ Proteção ativada no seu computador."
                        : "• Ainda não deu para ativar a proteção agora (o programa precisa de permissão de administrador).");

                    publish("Conferindo se a proteção está funcionando…");
                    List<ResultadoVerificacao> resultados = plataforma.verificarProtecao();
                    testesTotal = resultados.size();
                    for (ResultadoVerificacao r : resultados) {
                        if (r.isSucesso()) {
                            testesOk++;
                        }
                    }
                    publish("✓ Conferência concluída (" + testesOk + " de " + testesTotal + " verificações OK).");

                    if (!navegadores.isEmpty()) {
                        publish("Preparando o bloqueador de anúncios em " + nomesNavegadores() + "…");
                        for (Navegador nav : navegadores) {
                            boolean ok = plataforma.prepararNavegador(nav);
                            nav.setExtensaoInstalada(ok);
                        }
                        publish("✓ Bloqueador de anúncios preparado.");
                    }
                } catch (Exception ex) {
                    System.out.println("Erro técnico: " + ex.getMessage());
                    publish("• Tivemos um problema inesperado. Veja \"Detalhes técnicos\" para mais informações.");
                } finally {
                    System.setOut(original);
                    System.setErr(originalErr);
                }
                return null;
            }

            @Override
            protected void process(List<String> mensagens) {
                for (String m : mensagens) {
                    progresso(m);
                }
            }

            @Override
            protected void done() {
                progresso("");
                if (dnsOk) {
                    progresso("✓ Pronto! Seu computador está mais protegido.");
                } else {
                    progresso("Quase lá! Para concluir, " + plataforma.instrucoesAdmin() + ".");
                }
                if (!navegadores.isEmpty()) {
                    progresso("Dica: falta um último ajuste dentro do seu navegador para a proteção "
                        + "valer sempre. Se precisar, abra \"Detalhes técnicos\" para ver o passo a passo.");
                }
                progresso("Você pode desfazer tudo quando quiser.");
                botaoProteger.setText("Proteger meu computador");
                botaoProteger.setEnabled(true);
            }
        }.execute();
    }

    private String nomesNavegadores() {
        List<String> nomes = new ArrayList<>();
        for (Navegador nav : navegadores) {
            if (!nomes.contains(nav.getNome())) {
                nomes.add(nav.getNome());
            }
        }
        return String.join(", ", nomes);
    }

    private List<Navegador> deduplicar(List<Navegador> lista) {
        Map<String, Navegador> unicos = new LinkedHashMap<>();
        for (Navegador nav : lista) {
            unicos.putIfAbsent(nav.getNome() + "|" + nav.getTipo(), nav);
        }
        return new ArrayList<>(unicos.values());
    }

    private void progresso(String texto) {
        if (areaProgresso == null) {
            return;
        }
        SwingUtilities.invokeLater(() -> {
            areaProgresso.append(texto + "\n");
            areaProgresso.setCaretPosition(areaProgresso.getDocument().getLength());
        });
    }

    /** Redireciona a saída técnica dos serviços para o painel "Detalhes técnicos". */
    private class AreaTecnicaOutputStream extends java.io.OutputStream {
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
                areaTecnica.append(linha + "\n");
                areaTecnica.setCaretPosition(areaTecnica.getDocument().getLength());
            });
        }
    }
}

package br.uniube.pi.guialar.gui;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Locale;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Escala da interface no Linux. O Java não lê a escala fracionária do GNOME/KDE
 * (125%, 150%...), então em telas com escala a janela aparecia minúscula.
 * Precisa rodar antes de qualquer classe gráfica ser carregada.
 */
public final class EscalaTela {

    static final double MIN = 1.0;
    static final double MAX = 3.0;

    private static final Pattern XFT_DPI = Pattern.compile("(?m)^Xft\\.dpi:\\s*([0-9.]+)");
    private static final Pattern MONITOR_SCALE = Pattern.compile("<scale>\\s*([0-9.]+)\\s*</scale>");

    private EscalaTela() {
    }

    private static double fatorFonte = 1.0;

    /**
     * No Linux o Java só aceita {@code sun.java2d.uiScale} inteiro (1.5 é ignorado).
     * A parte inteira vai para o uiScale e a fracionária para as fontes da GUI.
     */
    public static void aplicar(String[] args) {
        if (!System.getProperty("os.name", "").toLowerCase(Locale.ROOT).contains("linux")) {
            return;
        }
        if (System.getProperty("sun.java2d.uiScale") != null || System.getenv("GDK_SCALE") != null) {
            return;
        }
        double escala = detectar(args);
        int inteira = (int) Math.floor(escala);
        if (inteira >= 2) {
            System.setProperty("sun.java2d.uiScale", Integer.toString(inteira));
        }
        fatorFonte = escala / Math.max(1, inteira);
    }

    /** Multiplicador das fontes da GUI (1.0 quando não há escala fracionária). */
    public static double fatorFonte() {
        return fatorFonte;
    }

    public static float fonte(float tamanho) {
        return (float) (tamanho * fatorFonte);
    }

    static double detectar(String[] args) {
        Double manual = lerManual(args, System.getenv("GUIALAR_ESCALA"));
        if (manual != null) {
            return manual;
        }
        Double xft = escalaDeXftDpi(executar("xrdb", "-query"));
        if (xft != null) {
            return xft;
        }
        String mutter = executar("gsettings", "get", "org.gnome.mutter", "experimental-features");
        if (mutter != null && mutter.contains("xwayland-native-scaling")) {
            try {
                Path monitores = Path.of(System.getProperty("user.home"), ".config", "monitors.xml");
                Double m = maiorEscalaMonitores(Files.readString(monitores));
                if (m != null) {
                    return m;
                }
            } catch (Exception ignored) {
                // sem monitors.xml: segue para o próximo método
            }
        }
        Double texto = numero(executar("gsettings", "get", "org.gnome.desktop.interface", "text-scaling-factor"));
        return texto != null ? limitar(texto) : MIN;
    }

    public static Double lerManual(String[] args, String env) {
        for (String a : args) {
            if (a.startsWith("--escala=")) {
                Double v = numero(a.substring("--escala=".length()));
                if (v != null) {
                    return limitar(v);
                }
            }
        }
        Double v = numero(env);
        return v != null ? limitar(v) : null;
    }

    public static Double escalaDeXftDpi(String xrdb) {
        if (xrdb == null) {
            return null;
        }
        Matcher m = XFT_DPI.matcher(xrdb);
        if (!m.find()) {
            return null;
        }
        double dpi = Double.parseDouble(m.group(1));
        return dpi > 0 ? limitar(dpi / 96.0) : null;
    }

    public static Double maiorEscalaMonitores(String xml) {
        if (xml == null) {
            return null;
        }
        Matcher m = MONITOR_SCALE.matcher(xml);
        Double maior = null;
        while (m.find()) {
            double v = Double.parseDouble(m.group(1));
            if (maior == null || v > maior) {
                maior = v;
            }
        }
        return maior != null ? limitar(maior) : null;
    }

    private static Double numero(String s) {
        if (s == null) {
            return null;
        }
        try {
            double v = Double.parseDouble(s.trim().replace(',', '.'));
            return v > 0 ? v : null;
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static double limitar(double v) {
        return Math.max(MIN, Math.min(MAX, v));
    }

    private static String executar(String... cmd) {
        try {
            Process p = new ProcessBuilder(cmd).redirectErrorStream(true).start();
            if (!p.waitFor(2, TimeUnit.SECONDS)) {
                p.destroyForcibly();
                return null;
            }
            String out = new String(p.getInputStream().readAllBytes());
            return p.exitValue() == 0 ? out : null;
        } catch (Exception e) {
            return null;
        }
    }
}

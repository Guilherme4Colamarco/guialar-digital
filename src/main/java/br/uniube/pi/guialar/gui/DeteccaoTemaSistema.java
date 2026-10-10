package br.uniube.pi.guialar.gui;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Detecta preferência claro/escuro do sistema (sem gravar override do usuário).
 */
public final class DeteccaoTemaSistema {

    private static final Pattern PORTAL_VARIANT = Pattern.compile("variant\\s+uint32\\s+(\\d+)");
    private static final Pattern GSETTINGS = Pattern.compile("'([^']*)'");

    private DeteccaoTemaSistema() {
    }

    public static Tema detectar() {
        String os = System.getProperty("os.name", "").toLowerCase();
        if (os.contains("win")) {
            return detectarWindows() ? Tema.ESCURO : Tema.CLARO;
        }
        Tema portal = detectarPortalFreedesktop();
        if (portal != null) {
            return portal;
        }
        Tema gnome = detectarGsettingsColorScheme();
        if (gnome != null) {
            return gnome;
        }
        return Tema.CLARO;
    }

    private static Tema detectarPortalFreedesktop() {
        String out = executarComando(3, "gdbus", "call", "--session",
            "--dest", "org.freedesktop.portal.Desktop",
            "--object-path", "/org/freedesktop/portal/desktop",
            "--method", "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance", "color-scheme");
        if (out == null) {
            return null;
        }
        Matcher m = PORTAL_VARIANT.matcher(out);
        if (!m.find()) {
            return null;
        }
        int scheme = Integer.parseInt(m.group(1));
        if (scheme == 1) {
            return Tema.ESCURO;
        }
        if (scheme == 2) {
            return Tema.CLARO;
        }
        return null;
    }

    private static Tema detectarGsettingsColorScheme() {
        String out = executarComando(3, "gsettings", "get",
            "org.gnome.desktop.interface", "color-scheme");
        if (out == null) {
            return null;
        }
        Matcher m = GSETTINGS.matcher(out.trim());
        if (!m.find()) {
            return null;
        }
        String valor = m.group(1).toLowerCase();
        if (valor.contains("dark")) {
            return Tema.ESCURO;
        }
        if (valor.contains("default") || valor.contains("light")) {
            return Tema.CLARO;
        }
        return null;
    }

    private static boolean detectarWindows() {
        try {
            Process p = new ProcessBuilder("reg", "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                "/v", "AppsUseLightTheme")
                .redirectErrorStream(true)
                .start();
            String out = lerSaida(p, 3);
            if (out == null) {
                return false;
            }
            if (out.contains("0x0")) {
                return true;
            }
            return out.trim().endsWith("0x0");
        } catch (Exception e) {
            return false;
        }
    }

    private static String executarComando(int timeoutSec, String... cmd) {
        try {
            Process p = new ProcessBuilder(cmd).redirectErrorStream(true).start();
            return lerSaida(p, timeoutSec);
        } catch (Exception e) {
            return null;
        }
    }

    private static String lerSaida(Process p, int timeoutSec) {
        try {
            if (!p.waitFor(timeoutSec, TimeUnit.SECONDS)) {
                p.destroyForcibly();
                return null;
            }
            if (p.exitValue() != 0) {
                return null;
            }
            try (BufferedReader br = new BufferedReader(
                new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8))) {
                StringBuilder sb = new StringBuilder();
                String line;
                while ((line = br.readLine()) != null) {
                    sb.append(line).append('\n');
                }
                return sb.toString();
            }
        } catch (Exception e) {
            return null;
        }
    }
}

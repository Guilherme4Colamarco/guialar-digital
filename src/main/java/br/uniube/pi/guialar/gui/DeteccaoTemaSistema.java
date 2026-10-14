package br.uniube.pi.guialar.gui;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Detecta preferência claro/escuro do sistema (sem gravar override do usuário).
 */
public final class DeteccaoTemaSistema {

    private static final int TIMEOUT_TOTAL_SEC = 2;
    private static final Pattern PORTAL_VARIANT = Pattern.compile(
        "(?:variant\\s+uint32\\s+|<<uint32\\s+)(\\d+)", Pattern.CASE_INSENSITIVE);
    private static final Pattern GSETTINGS = Pattern.compile("'([^']*)'");
    private static final Pattern REG_APPS_LIGHT = Pattern.compile(
        "AppsUseLightTheme\\s+REG_DWORD\\s+0x([0-9a-fA-F]+)", Pattern.CASE_INSENSITIVE);

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
        String out = executarComando("gdbus", "call", "--session",
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
        String out = executarComando("gsettings", "get",
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
        String out = executarComando("reg", "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            "/v", "AppsUseLightTheme");
        if (out == null) {
            return false;
        }
        Matcher m = REG_APPS_LIGHT.matcher(out);
        if (!m.find()) {
            return false;
        }
        return Integer.parseInt(m.group(1), 16) == 0;
    }

    private static String executarComando(String... cmd) {
        try {
            Process p = new ProcessBuilder(cmd).redirectErrorStream(true).start();
            StringBuilder sb = new StringBuilder();
            Thread leitor = new Thread(() -> lerStream(p, sb), "guialar-tema-detect");
            leitor.setDaemon(true);
            leitor.start();
            if (!p.waitFor(TIMEOUT_TOTAL_SEC, TimeUnit.SECONDS)) {
                p.destroyForcibly();
                return null;
            }
            leitor.join(400);
            if (p.exitValue() != 0) {
                return null;
            }
            return sb.toString();
        } catch (Exception e) {
            return null;
        }
    }

    private static void lerStream(Process p, StringBuilder sb) {
        try (BufferedReader br = new BufferedReader(
            new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8))) {
            String line;
            while ((line = br.readLine()) != null) {
                sb.append(line).append('\n');
            }
        } catch (IOException ignored) {
        }
    }
}

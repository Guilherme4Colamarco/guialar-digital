package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Abre URLs no navegador padrão do Windows (equivalente a xdg-open).
 * Em não-Windows tenta xdg-open / open como fallback de desenvolvimento.
 */
public class WindowsUrlOpener {

    private final PowerShellExecutor executor;

    public WindowsUrlOpener() {
        this(new ProcessPowerShellExecutor());
    }

    public WindowsUrlOpener(PowerShellExecutor executor) {
        this.executor = executor;
    }

    public boolean abrir(String url) {
        if (url == null || url.isBlank()) {
            return false;
        }
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        try {
            if (os.contains("win")) {
                PowerShellResult r = executor.executar(
                    "Start-Process '" + url.replace("'", "''") + "'");
                if (r.isOk()) {
                    return true;
                }
                Process p = new ProcessBuilder("cmd", "/c", "start", "", url)
                    .redirectErrorStream(true)
                    .start();
                return p.waitFor() == 0;
            }
            if (os.contains("mac")) {
                return new ProcessBuilder("open", url).start().waitFor() == 0;
            }
            return new ProcessBuilder("xdg-open", url).start().waitFor() == 0;
        } catch (Exception e) {
            return false;
        }
    }

    /**
     * Textos de orientação por navegador (português), sem abrir nada.
     */
    public static List<String> guiasPara(String nomeNavegador) {
        List<String> linhas = new ArrayList<>();
        String n = nomeNavegador == null ? "" : nomeNavegador.toLowerCase(Locale.ROOT);

        if (n.contains("brave")) {
            linhas.add("Brave: ative os Shields (ícone do leão) no site.");
            linhas.add("Brave → Configurações → Privacidade → DNS seguro:");
            linhas.add("  use https://family.cloudflare-dns.com/dns-query");
            linhas.add("Loja de extensões (opcional): https://chromewebstore.google.com/");
            return linhas;
        }
        if (n.contains("firefox") || n.contains("mozilla")) {
            linhas.add("Firefox: instale uBlock Origin:");
            linhas.add("  https://addons.mozilla.org/firefox/addon/ublock-origin/");
            linhas.add("DoH: about:preferences#privacy → DNS sobre HTTPS →");
            linhas.add("  https://family.cloudflare-dns.com/dns-query");
            return linhas;
        }
        if (n.contains("edge")) {
            linhas.add("Microsoft Edge: instale uBlock Origin Lite:");
            linhas.add("  https://microsoftedge.microsoft.com/addons/detail/ublock-origin-lite/cimiefiiaegbelhefglklhhakcgjbaej");
            linhas.add("(ou Chrome Web Store: uBlock Origin Lite)");
            linhas.add("DoH: edge://settings/privacy → Usar DNS seguro →");
            linhas.add("  https://family.cloudflare-dns.com/dns-query");
            return linhas;
        }
        if (n.contains("chrome") || n.contains("chromium")) {
            linhas.add("Chrome: instale uBlock Origin Lite:");
            linhas.add("  https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh");
            linhas.add("DoH: chrome://settings/security → Usar DNS seguro →");
            linhas.add("  https://family.cloudflare-dns.com/dns-query");
            return linhas;
        }

        linhas.add("Abra o navegador e configure DNS seguro (DoH) para:");
        linhas.add("  https://family.cloudflare-dns.com/dns-query");
        linhas.add("Instale uBlock Origin (Firefox) ou uBlock Origin Lite (Chromium/Edge).");
        return linhas;
    }

    public static String urlLojaPadrao(String nomeNavegador) {
        String n = nomeNavegador == null ? "" : nomeNavegador.toLowerCase(Locale.ROOT);
        if (n.contains("firefox") || n.contains("mozilla")) {
            return "https://addons.mozilla.org/firefox/addon/ublock-origin/";
        }
        if (n.contains("edge")) {
            return "https://microsoftedge.microsoft.com/addons/detail/ublock-origin-lite/cimiefiiaegbelhefglklhhakcgjbaej";
        }
        if (n.contains("brave")) {
            return "https://support.brave.com/hc/en-us/articles/360023646212-How-do-I-configure-Shields";
        }
        return "https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh";
    }
}

package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.adaptadores.BrowserDetector;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Detector de navegadores no Linux (PATH + perfis em ~/.config / ~/.mozilla).
 */
public class LinuxBrowserDetector implements BrowserDetector {

    private final String home;

    public LinuxBrowserDetector() {
        this(System.getProperty("user.home"));
    }

    public LinuxBrowserDetector(String home) {
        this.home = home;
    }

    @Override
    public List<Navegador> detectar() {
        List<Navegador> navegadores = new ArrayList<>();
        navegadores.addAll(detectarFirefox());
        navegadores.addAll(detectarChromium());
        return navegadores;
    }

    @Override
    public boolean isInstalado(String nome) {
        if (nome == null) {
            return false;
        }
        String alvo = nome.toLowerCase();
        return detectar().stream()
            .anyMatch(n -> n.getNome().toLowerCase().contains(alvo)
                || n.getCaminhoExecutavel().toLowerCase().contains(alvo));
    }

    private List<Navegador> detectarFirefox() {
        List<Navegador> navegadores = new ArrayList<>();
        String[][] firefoxVariantes = {
            {"firefox", "Firefox", ".mozilla/firefox"},
            {"firefox-esr", "Firefox ESR", ".mozilla/firefox"},
            {"librewolf", "LibreWolf", ".librewolf"},
            {"waterfox", "Waterfox", ".waterfox"}
        };
        for (String[] variante : firefoxVariantes) {
            String executavel = variante[0];
            String nome = variante[1];
            String caminhoPerfil = home + "/" + variante[2];
            if (comandoExiste(executavel) || Files.exists(Path.of(caminhoPerfil))) {
                navegadores.add(new Navegador(nome, TipoNavegador.FIREFOX, executavel, caminhoPerfil, false));
            }
        }
        return navegadores;
    }

    private List<Navegador> detectarChromium() {
        List<Navegador> navegadores = new ArrayList<>();
        String[][] chromiumVariantes = {
            {"chromium", "Chromium", ".config/chromium"},
            {"chromium-browser", "Chromium", ".config/chromium"},
            {"google-chrome", "Google Chrome", ".config/google-chrome"},
            {"google-chrome-stable", "Google Chrome", ".config/google-chrome"},
            {"brave", "Brave", ".config/BraveSoftware/Brave-Browser"},
            {"brave-browser", "Brave", ".config/BraveSoftware/Brave-Browser"},
            {"microsoft-edge", "Microsoft Edge", ".config/microsoft-edge"},
            {"vivaldi", "Vivaldi", ".config/vivaldi"},
            {"opera", "Opera", ".config/opera"}
        };
        for (String[] variante : chromiumVariantes) {
            String executavel = variante[0];
            String nome = variante[1];
            String caminhoPerfil = home + "/" + variante[2];
            if (comandoExiste(executavel) || Files.exists(Path.of(caminhoPerfil))) {
                navegadores.add(new Navegador(nome, TipoNavegador.CHROMIUM, executavel, caminhoPerfil, false));
            }
        }
        return navegadores;
    }

    private boolean comandoExiste(String comando) {
        try {
            Process process = new ProcessBuilder("which", comando)
                .redirectErrorStream(true)
                .start();
            return process.waitFor() == 0;
        } catch (Exception e) {
            return false;
        }
    }
}

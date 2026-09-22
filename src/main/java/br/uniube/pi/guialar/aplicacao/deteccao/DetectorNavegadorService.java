package br.uniube.pi.guialar.aplicacao.deteccao;

import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Serviço para detecção de navegadores instalados.
 * Detecta navegadores base Firefox e base Chromium.
 */
public class DetectorNavegadorService {

    private static final String HOME = System.getProperty("user.home");

    /**
     * Detecta todos os navegadores instalados no sistema.
     */
    public List<Navegador> detectar() {
        List<Navegador> navegadores = new ArrayList<>();
        
        navegadores.addAll(detectarFirefox());
        navegadores.addAll(detectarChromium());
        
        return navegadores;
    }

    /**
     * Detecta navegadores base Firefox (Gecko engine).
     */
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
            String caminhoPerfil = HOME + "/" + variante[2];

            if (comandoExiste(executavel) || Files.exists(Path.of(caminhoPerfil))) {
                navegadores.add(new Navegador(
                    nome,
                    TipoNavegador.FIREFOX,
                    executavel,
                    caminhoPerfil,
                    false
                ));
            }
        }

        return navegadores;
    }

    /**
     * Detecta navegadores base Chromium (Blink engine).
     */
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
            String caminhoPerfil = HOME + "/" + variante[2];

            if (comandoExiste(executavel) || Files.exists(Path.of(caminhoPerfil))) {
                navegadores.add(new Navegador(
                    nome,
                    TipoNavegador.CHROMIUM,
                    executavel,
                    caminhoPerfil,
                    false
                ));
            }
        }

        return navegadores;
    }

    private boolean comandoExiste(String comando) {
        try {
            Process process = new ProcessBuilder("which", comando)
                .redirectErrorStream(true)
                .start();
            int exitCode = process.waitFor();
            return exitCode == 0;
        } catch (Exception e) {
            return false;
        }
    }
}

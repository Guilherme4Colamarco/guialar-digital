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
     * Usa múltiplos métodos: which, perfis, .desktop, Flatpak, Snap
     */
    public List<Navegador> detectar() {
        List<Navegador> navegadores = new ArrayList<>();
        
        navegadores.addAll(detectarFirefox());
        navegadores.addAll(detectarChromium());
        
        // Deduplicate por nome
        List<Navegador> unicos = new ArrayList<>();
        for (Navegador nav : navegadores) {
            boolean jaExiste = false;
            for (Navegador unico : unicos) {
                if (unico.getNome().equals(nav.getNome()) && 
                    unico.getCaminhoPerfil().equals(nav.getCaminhoPerfil())) {
                    jaExiste = true;
                    break;
                }
            }
            if (!jaExiste) {
                unicos.add(nav);
            }
        }
        
        return unicos;
    }

    /**
     * Detecta navegadores base Firefox (Gecko engine).
     * Verifica: which, perfis, .desktop, Flatpak, Snap
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

            boolean detectado = false;
            
            // Método 1: which
            if (comandoExiste(executavel)) {
                detectado = true;
            }
            
            // Método 2: Perfil existe
            if (!detectado && Files.exists(Path.of(caminhoPerfil))) {
                detectado = true;
            }
            
            // Método 3: .desktop files
            if (!detectado && desktopFileExiste(executavel)) {
                detectado = true;
            }
            
            // Método 4: Flatpak
            if (!detectado && flatpakInstalado("firefox")) {
                caminhoPerfil = HOME + "/.var/app/org.mozilla.firefox/.mozilla/firefox";
                detectado = true;
            }
            
            // Método 5: Snap
            if (!detectado && snapInstalado("firefox")) {
                caminhoPerfil = HOME + "/snap/firefox/common/.mozilla/firefox";
                detectado = true;
            }

            if (detectado) {
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
     * Verifica: which, perfis, .desktop, Flatpak, Snap, Brave Origin
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

            boolean detectado = false;
            
            // Método 1: which
            if (comandoExiste(executavel)) {
                detectado = true;
            }
            
            // Método 2: Perfil existe
            if (!detectado && Files.exists(Path.of(caminhoPerfil))) {
                detectado = true;
            }
            
            // Método 3: .desktop files
            if (!detectado && desktopFileExiste(executavel)) {
                detectado = true;
            }
            
            // Método 4: Flatpak
            String flatpakId = null;
            if (executavel.contains("chrome")) {
                flatpakId = "com.google.Chrome";
            } else if (executavel.contains("chromium")) {
                flatpakId = "org.chromium.Chromium";
            } else if (executavel.contains("brave")) {
                flatpakId = "com.brave.Browser";
            }
            
            if (!detectado && flatpakId != null && flatpakInstalado(flatpakId)) {
                caminhoPerfil = HOME + "/.var/app/" + flatpakId + "/config/" + variante[2].replace(".config/", "");
                detectado = true;
            }
            
            // Método 5: Snap
            if (!detectado && snapInstalado(executavel)) {
                caminhoPerfil = HOME + "/snap/" + executavel + "/common/" + variante[2];
                detectado = true;
            }

            if (detectado) {
                navegadores.add(new Navegador(
                    nome,
                    TipoNavegador.CHROMIUM,
                    executavel,
                    caminhoPerfil,
                    false
                ));
            }
        }
        
        // Brave Origin (caminho diferente)
        String braveOriginPerfil = HOME + "/.config/BraveSoftware/Brave-Origin";
        if (Files.exists(Path.of(braveOriginPerfil))) {
            navegadores.add(new Navegador(
                "Brave Origin",
                TipoNavegador.CHROMIUM,
                "brave-origin",
                braveOriginPerfil,
                false
            ));
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
    
    private boolean desktopFileExiste(String nomeApp) {
        // Verificar em /usr/share/applications/
        Path systemDesktop = Path.of("/usr/share/applications", nomeApp + ".desktop");
        if (Files.exists(systemDesktop)) {
            return true;
        }
        
        // Verificar em ~/.local/share/applications/
        Path userDesktop = Path.of(HOME, ".local/share/applications", nomeApp + ".desktop");
        if (Files.exists(userDesktop)) {
            return true;
        }
        
        // Verificar variações comuns
        String[] sufixos = {"", "-stable", "-browser", "-nightly"};
        for (String sufixo : sufixos) {
            Path variacaoSystem = Path.of("/usr/share/applications", nomeApp + sufixo + ".desktop");
            Path variacaoUser = Path.of(HOME, ".local/share/applications", nomeApp + sufixo + ".desktop");
            
            if (Files.exists(variacaoSystem) || Files.exists(variacaoUser)) {
                return true;
            }
        }
        
        return false;
    }
    
    private boolean flatpakInstalado(String appId) {
        try {
            Process process = new ProcessBuilder("flatpak", "list", "--app")
                .redirectErrorStream(true)
                .start();
            
            java.io.BufferedReader reader = new java.io.BufferedReader(
                new java.io.InputStreamReader(process.getInputStream())
            );
            
            String linha;
            while ((linha = reader.readLine()) != null) {
                if (linha.toLowerCase().contains(appId.toLowerCase())) {
                    return true;
                }
            }
            
            process.waitFor();
            return false;
        } catch (Exception e) {
            return false;
        }
    }
    
    private boolean snapInstalado(String snapName) {
        try {
            Process process = new ProcessBuilder("snap", "list", snapName)
                .redirectErrorStream(true)
                .start();
            int exitCode = process.waitFor();
            return exitCode == 0;
        } catch (Exception e) {
            return false;
        }
    }
}

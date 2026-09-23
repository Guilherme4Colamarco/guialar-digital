package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.dominio.adaptadores.BrowserDetector;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Detecção de navegadores no Windows.
 *
 * Procura os executáveis nos caminhos padrão de instalação (Program Files e
 * pastas de dados do usuário em %LOCALAPPDATA%/%APPDATA%). Para cada navegador
 * detectado guarda também a pasta de perfil, usada na etapa de extensões.
 */
public class WindowsBrowserDetector implements BrowserDetector {

    private final String programFiles = env("ProgramFiles", "C:\\Program Files");
    private final String programFilesX86 = env("ProgramFiles(x86)", "C:\\Program Files (x86)");
    private final String localAppData = env("LOCALAPPDATA", "");
    private final String appData = env("APPDATA", "");

    @Override
    public List<Navegador> detectar() {
        List<Navegador> navegadores = new ArrayList<>();

        adicionarChromium(navegadores, "Google Chrome",
            new String[]{
                programFiles + "\\Google\\Chrome\\Application\\chrome.exe",
                programFilesX86 + "\\Google\\Chrome\\Application\\chrome.exe",
                localAppData + "\\Google\\Chrome\\Application\\chrome.exe"},
            localAppData + "\\Google\\Chrome\\User Data");

        adicionarChromium(navegadores, "Microsoft Edge",
            new String[]{
                programFilesX86 + "\\Microsoft\\Edge\\Application\\msedge.exe",
                programFiles + "\\Microsoft\\Edge\\Application\\msedge.exe"},
            localAppData + "\\Microsoft\\Edge\\User Data");

        adicionarChromium(navegadores, "Brave",
            new String[]{
                programFiles + "\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                programFilesX86 + "\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
                localAppData + "\\BraveSoftware\\Brave-Browser\\Application\\brave.exe"},
            localAppData + "\\BraveSoftware\\Brave-Browser\\User Data");

        adicionarChromium(navegadores, "Opera",
            new String[]{localAppData + "\\Programs\\Opera\\opera.exe"},
            appData + "\\Opera Software\\Opera Stable");

        adicionarChromium(navegadores, "Vivaldi",
            new String[]{
                localAppData + "\\Vivaldi\\Application\\vivaldi.exe",
                programFiles + "\\Vivaldi\\Application\\vivaldi.exe"},
            localAppData + "\\Vivaldi\\User Data");

        adicionarFirefox(navegadores, "Mozilla Firefox",
            new String[]{
                programFiles + "\\Mozilla Firefox\\firefox.exe",
                programFilesX86 + "\\Mozilla Firefox\\firefox.exe"},
            appData + "\\Mozilla\\Firefox\\Profiles");

        return navegadores;
    }

    @Override
    public boolean isInstalado(String nome) {
        for (Navegador nav : detectar()) {
            if (nav.getNome().equalsIgnoreCase(nome)) {
                return true;
            }
        }
        return false;
    }

    private void adicionarChromium(List<Navegador> lista, String nome, String[] executaveis, String perfil) {
        String exe = primeiroExistente(executaveis);
        if (exe != null) {
            lista.add(new Navegador(nome, TipoNavegador.CHROMIUM, exe, perfil, false));
        }
    }

    private void adicionarFirefox(List<Navegador> lista, String nome, String[] executaveis, String perfil) {
        String exe = primeiroExistente(executaveis);
        if (exe != null) {
            lista.add(new Navegador(nome, TipoNavegador.FIREFOX, exe, perfil, false));
        }
    }

    private String primeiroExistente(String[] caminhos) {
        for (String caminho : caminhos) {
            if (caminho != null && !caminho.isBlank()) {
                try {
                    if (Files.exists(Path.of(caminho))) {
                        return caminho;
                    }
                } catch (Exception ignored) {
                    // Caminho inválido: ignora e tenta o próximo.
                }
            }
        }
        return null;
    }

    private static String env(String chave, String padrao) {
        String v = System.getenv(chave);
        return (v == null || v.isBlank()) ? padrao : v;
    }
}

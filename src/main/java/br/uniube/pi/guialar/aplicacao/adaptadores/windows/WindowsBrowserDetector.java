package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.dominio.adaptadores.BrowserDetector;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Detecta Edge, Chrome, Firefox e Brave no Windows via caminhos comuns
 * e, quando possível, via registro (somente leitura).
 */
public class WindowsBrowserDetector implements BrowserDetector {

    private final PowerShellExecutor executor;
    private final Path programFiles;
    private final Path programFilesX86;
    private final Path localAppData;
    private final Path appData;

    public WindowsBrowserDetector() {
        this(new ProcessPowerShellExecutor());
    }

    public WindowsBrowserDetector(PowerShellExecutor executor) {
        this.executor = executor;
        this.programFiles = Path.of(envOu("ProgramFiles", "C:\\Program Files"));
        this.programFilesX86 = Path.of(envOu("ProgramFiles(x86)", "C:\\Program Files (x86)"));
        this.localAppData = Path.of(envOu("LOCALAPPDATA",
            System.getProperty("user.home") + "/AppData/Local"));
        this.appData = Path.of(envOu("APPDATA",
            System.getProperty("user.home") + "/AppData/Roaming"));
    }

    /** Construtor para testes com caminhos injetados. */
    public WindowsBrowserDetector(PowerShellExecutor executor,
                           String programFiles, String programFilesX86,
                           String localAppData, String appData) {
        this.executor = executor;
        this.programFiles = Path.of(programFiles);
        this.programFilesX86 = Path.of(programFilesX86);
        this.localAppData = Path.of(localAppData);
        this.appData = Path.of(appData);
    }

    @Override
    public List<Navegador> detectar() {
        Map<String, Navegador> unicos = new LinkedHashMap<>();

        adicionarSeExiste(unicos, "Microsoft Edge", TipoNavegador.CHROMIUM,
            List.of(
                programFiles.resolve(Path.of("Microsoft", "Edge", "Application", "msedge.exe")),
                programFilesX86.resolve(Path.of("Microsoft", "Edge", "Application", "msedge.exe"))
            ),
            localAppData.resolve(Path.of("Microsoft", "Edge", "User Data")));

        adicionarSeExiste(unicos, "Google Chrome", TipoNavegador.CHROMIUM,
            List.of(
                programFiles.resolve(Path.of("Google", "Chrome", "Application", "chrome.exe")),
                programFilesX86.resolve(Path.of("Google", "Chrome", "Application", "chrome.exe"))
            ),
            localAppData.resolve(Path.of("Google", "Chrome", "User Data")));

        adicionarSeExiste(unicos, "Mozilla Firefox", TipoNavegador.FIREFOX,
            List.of(
                programFiles.resolve(Path.of("Mozilla Firefox", "firefox.exe")),
                programFilesX86.resolve(Path.of("Mozilla Firefox", "firefox.exe"))
            ),
            appData.resolve(Path.of("Mozilla", "Firefox")));

        adicionarSeExiste(unicos, "Brave", TipoNavegador.CHROMIUM,
            List.of(
                programFiles.resolve(Path.of("BraveSoftware", "Brave-Browser", "Application", "brave.exe")),
                programFilesX86.resolve(Path.of("BraveSoftware", "Brave-Browser", "Application", "brave.exe")),
                localAppData.resolve(Path.of("BraveSoftware", "Brave-Browser", "Application", "brave.exe"))
            ),
            localAppData.resolve(Path.of("BraveSoftware", "Brave-Browser", "User Data")));

        for (Navegador viaReg : detectarViaRegistro()) {
            unicos.putIfAbsent(viaReg.getNome(), viaReg);
        }

        return new ArrayList<>(unicos.values());
    }

    @Override
    public boolean isInstalado(String nome) {
        if (nome == null) {
            return false;
        }
        String alvo = nome.toLowerCase();
        return detectar().stream()
            .anyMatch(n -> n.getNome().toLowerCase().contains(alvo)
                || (n.getCaminhoExecutavel() != null
                    && n.getCaminhoExecutavel().toLowerCase().contains(alvo)));
    }

    private void adicionarSeExiste(Map<String, Navegador> unicos, String nome,
                                   TipoNavegador tipo, List<Path> executaveis,
                                   Path perfil) {
        for (Path exe : executaveis) {
            if (Files.isRegularFile(exe)) {
                unicos.putIfAbsent(nome, new Navegador(
                    nome, tipo, exe.toString(), perfil.toString(), false));
                return;
            }
        }
        if (Files.isDirectory(perfil)) {
            unicos.putIfAbsent(nome, new Navegador(
                nome, tipo, "", perfil.toString(), false));
        }
    }

    private List<Navegador> detectarViaRegistro() {
        List<Navegador> lista = new ArrayList<>();
        PowerShellResult r = executor.executar("""
            $ErrorActionPreference = 'SilentlyContinue'
            $paths = @(
              'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\msedge.exe',
              'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe',
              'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\firefox.exe',
              'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\brave.exe',
              'HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\msedge.exe',
              'HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\chrome.exe',
              'HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\firefox.exe',
              'HKCU:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\brave.exe'
            )
            foreach ($p in $paths) {
              $v = (Get-ItemProperty -Path $p -ErrorAction SilentlyContinue).'(default)'
              if ($v) { Write-Output ("REG=" + $p + "|" + $v) }
            }
            """);
        if (!r.isOk()) {
            return lista;
        }
        for (String line : r.getStdout().split("\\R")) {
            if (!line.startsWith("REG=")) {
                continue;
            }
            String body = line.substring(4);
            int sep = body.indexOf('|');
            if (sep < 0) {
                continue;
            }
            String chave = body.substring(0, sep).toLowerCase();
            String exe = body.substring(sep + 1).trim();
            if (chave.contains("msedge")) {
                lista.add(new Navegador("Microsoft Edge", TipoNavegador.CHROMIUM, exe,
                    localAppData.resolve(Path.of("Microsoft", "Edge", "User Data")).toString(), false));
            } else if (chave.contains("chrome")) {
                lista.add(new Navegador("Google Chrome", TipoNavegador.CHROMIUM, exe,
                    localAppData.resolve(Path.of("Google", "Chrome", "User Data")).toString(), false));
            } else if (chave.contains("firefox")) {
                lista.add(new Navegador("Mozilla Firefox", TipoNavegador.FIREFOX, exe,
                    appData.resolve(Path.of("Mozilla", "Firefox")).toString(), false));
            } else if (chave.contains("brave")) {
                lista.add(new Navegador("Brave", TipoNavegador.CHROMIUM, exe,
                    localAppData.resolve(Path.of("BraveSoftware", "Brave-Browser", "User Data")).toString(), false));
            }
        }
        return lista;
    }

    private static String envOu(String chave, String padrao) {
        String v = System.getenv(chave);
        return (v == null || v.isBlank()) ? padrao : v;
    }
}

package br.uniube.pi.guialar.aplicacao.deteccao;

import br.uniube.pi.guialar.aplicacao.adaptadores.linux.LinuxBrowserDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsBrowserDetector;
import br.uniube.pi.guialar.dominio.adaptadores.BrowserDetector;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

import java.util.List;

/**
 * Fachada de detecção de navegadores: escolhe o adaptador Linux ou Windows.
 */
public class DetectorNavegadorService {

    private final DetectorSistemaService detectorSistema;
    private final BrowserDetector linuxDetector;
    private final BrowserDetector windowsDetector;

    public DetectorNavegadorService() {
        this(new DetectorSistemaService(), new LinuxBrowserDetector(), new WindowsBrowserDetector());
    }

    public DetectorNavegadorService(DetectorSistemaService detectorSistema,
                                    BrowserDetector linuxDetector,
                                    BrowserDetector windowsDetector) {
        this.detectorSistema = detectorSistema;
        this.linuxDetector = linuxDetector;
        this.windowsDetector = windowsDetector;
    }

    public List<Navegador> detectar() {
        return adaptadorAtual().detectar();
    }

    public boolean isInstalado(String nome) {
        return adaptadorAtual().isInstalado(nome);
    }

    private BrowserDetector adaptadorAtual() {
        TipoSistema tipo = detectorSistema.detectar();
        if (tipo.isWindows()) {
            return windowsDetector;
        }
        return linuxDetector;
    }
}

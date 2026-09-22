package br.uniube.pi.guialar.aplicacao.deteccao;

import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

/**
 * Serviço para detecção do sistema operacional.
 * Detecta se é Linux ou Windows.
 */
public class DetectorSistemaService {

    /**
     * Detecta o sistema operacional atual.
     */
    public TipoSistema detectar() {
        String osName = System.getProperty("os.name").toLowerCase();
        
        if (osName.contains("linux")) {
            return TipoSistema.LINUX;
        } else if (osName.contains("windows")) {
            return TipoSistema.WINDOWS;
        }
        
        return TipoSistema.DESCONHECIDO;
    }
}

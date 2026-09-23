package br.uniube.pi.guialar.aplicacao.plataforma;

import br.uniube.pi.guialar.aplicacao.deteccao.DetectorSistemaService;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

/**
 * Escolhe a implementação de {@link PlataformaService} conforme o sistema atual.
 */
public final class PlataformaFactory {

    private PlataformaFactory() {
    }

    public static PlataformaService criar() {
        TipoSistema tipo = new DetectorSistemaService().detectar();
        if (tipo == TipoSistema.WINDOWS) {
            return new WindowsPlataforma();
        }
        return new LinuxPlataforma();
    }
}

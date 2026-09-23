package br.uniube.pi.guialar.dominio.sistema;

/**
 * Informações amigáveis sobre o sistema operacional detectado.
 *
 * Serve como abstração multiplataforma consumida pela GUI e pela CLI, no lugar
 * de estruturas específicas de um único sistema (como {@code InfoDistro}, que é
 * exclusiva do Linux). Assim o mesmo fluxo funciona em Linux e Windows.
 */
public class SistemaInfo {

    private final TipoSistema tipo;
    private final String nomeAmigavel;
    private final boolean compativel;
    private final String detalheTecnico;

    public SistemaInfo(TipoSistema tipo, String nomeAmigavel, boolean compativel, String detalheTecnico) {
        this.tipo = tipo;
        this.nomeAmigavel = nomeAmigavel;
        this.compativel = compativel;
        this.detalheTecnico = detalheTecnico;
    }

    public TipoSistema getTipo() {
        return tipo;
    }

    /** Nome legível para o usuário (ex.: "Ubuntu 24.04.4 LTS" ou "Windows 11"). */
    public String getNomeAmigavel() {
        return nomeAmigavel;
    }

    /** Indica se o GuiaLar consegue proteger este sistema. */
    public boolean isCompativel() {
        return compativel;
    }

    /** Detalhe técnico opcional (ex.: família/gerenciador no Linux, versão no Windows). */
    public String getDetalheTecnico() {
        return detalheTecnico;
    }
}

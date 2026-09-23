package br.uniube.pi.guialar.dominio.distro;

/**
 * Tipos de distribuições Linux suportadas pelo GuiaLar Digital.
 * MVP: Debian, Fedora e Arch Linux (a "trindade sagrada").
 */
public enum TipoDistro {
    DEBIAN("Debian", "debian"),
    FEDORA("Fedora", "fedora"),
    ARCH("Arch Linux", "arch"),
    DESCONHECIDA("Desconhecida", "unknown");

    private final String nomeExibicao;
    private final String identificador;

    TipoDistro(String nomeExibicao, String identificador) {
        this.nomeExibicao = nomeExibicao;
        this.identificador = identificador;
    }

    public String getNomeExibicao() {
        return nomeExibicao;
    }

    public String getIdentificador() {
        return identificador;
    }
}

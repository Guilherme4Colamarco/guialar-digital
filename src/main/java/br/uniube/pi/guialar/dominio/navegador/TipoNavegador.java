package br.uniube.pi.guialar.dominio.navegador;

/**
 * Tipos base de navegadores detectados.
 * Determina qual extensão de adblocker será instalada.
 */
public enum TipoNavegador {
    /**
     * Navegadores base Firefox (Gecko engine)
     * Exemplos: Firefox, Firefox ESR, Librewolf, Waterfox
     * Extensão: uBlock Origin (addon oficial)
     */
    FIREFOX("Firefox", "firefox"),

    /**
     * Navegadores base Chromium (Blink engine)
     * Exemplos: Chromium, Chrome, Brave, Edge, Vivaldi, Opera
     * Extensão: uBlock Origin Lite (Manifest V3)
     */
    CHROMIUM("Chromium", "chromium"),

    DESCONHECIDO("Desconhecido", "unknown");

    private final String nomeExibicao;
    private final String identificador;

    TipoNavegador(String nomeExibicao, String identificador) {
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

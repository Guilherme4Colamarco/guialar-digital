package br.uniube.pi.guialar.dominio.navegador;

/**
 * Extensões de adblocker suportadas.
 */
public enum ExtensaoAdblocker {
    /**
     * uBlock Origin - Para navegadores base Firefox
     * ID da extensão: uBlock0@raymondhill.net
     * URL: https://addons.mozilla.org/firefox/addon/ublock-origin/
     */
    UBLOCK_ORIGIN(
        "uBlock Origin",
        "uBlock0@raymondhill.net",
        "https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi",
        TipoNavegador.FIREFOX
    ),

    /**
     * uBlock Origin Lite - Para navegadores base Chromium (Manifest V3)
     * ID da extensão: ddkjiahejlhfcafbddmgiahcphecmpfh
     * URL: https://chrome.google.com/webstore/detail/ublock-origin-lite/
     */
    UBLOCK_ORIGIN_LITE(
        "uBlock Origin Lite",
        "ddkjiahejlhfcafbddmgiahcphecmpfh",
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion=49.0&x=id%3Dddkjiahejlhfcafbddmgiahcphecmpfh%26installsource%3Dondemand%26uc",
        TipoNavegador.CHROMIUM
    );

    private final String nome;
    private final String id;
    private final String urlDownload;
    private final TipoNavegador tipoNavegador;

    ExtensaoAdblocker(String nome, String id, String urlDownload, TipoNavegador tipoNavegador) {
        this.nome = nome;
        this.id = id;
        this.urlDownload = urlDownload;
        this.tipoNavegador = tipoNavegador;
    }

    public String getNome() {
        return nome;
    }

    public String getId() {
        return id;
    }

    public String getUrlDownload() {
        return urlDownload;
    }

    public TipoNavegador getTipoNavegador() {
        return tipoNavegador;
    }

    /**
     * Retorna a extensão apropriada para o tipo de navegador.
     */
    public static ExtensaoAdblocker paraNavegador(TipoNavegador tipo) {
        return switch (tipo) {
            case FIREFOX -> UBLOCK_ORIGIN;
            case CHROMIUM -> UBLOCK_ORIGIN_LITE;
            default -> null;
        };
    }
}

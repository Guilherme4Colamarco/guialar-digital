package br.uniube.pi.guialar.dominio.diagnostico;

/**
 * Estado da proteção DNS GuiaLar no sistema.
 * Nunca alegar "filtro ativo" se a escrita de DNS falhou.
 */
public enum StatusDns {
    APLICADO("aplicado"),
    NAO_APLICADO("não aplicado"),
    PARCIAL("parcial"),
    DESCONHECIDO("desconhecido"),
    LEITURA_BLOQUEADA("leitura bloqueada");

    private final String rotuloPt;

    StatusDns(String rotuloPt) {
        this.rotuloPt = rotuloPt;
    }

    public String getRotuloPt() {
        return rotuloPt;
    }
}

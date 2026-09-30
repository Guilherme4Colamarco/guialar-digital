package br.uniube.pi.guialar.dominio.sistema;

/**
 * Tipos de sistemas operacionais suportados.
 * 
 * MVP COMPLETO: Linux (Debian, Fedora, Arch)
 * ESBOÇO: Windows
 * FORA: macOS, Android
 */
public enum TipoSistema {
    LINUX("Linux"),
    WINDOWS("Windows"),
    DESCONHECIDO("Desconhecido");

    private final String nome;

    TipoSistema(String nome) {
        this.nome = nome;
    }

    public String getNome() {
        return nome;
    }

    public boolean isLinux() {
        return this == LINUX;
    }

    public boolean isWindows() {
        return this == WINDOWS;
    }
}

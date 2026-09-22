package br.uniube.pi.guialar.dominio.distro;

/**
 * Informações sobre a distribuição Linux detectada.
 */
public class InfoDistro {
    private TipoDistro tipo;
    private String nome;
    private String versao;
    private String gerenciadorRede;

    public InfoDistro(TipoDistro tipo, String nome, String versao, String gerenciadorRede) {
        this.tipo = tipo;
        this.nome = nome;
        this.versao = versao;
        this.gerenciadorRede = gerenciadorRede;
    }

    public TipoDistro getTipo() {
        return tipo;
    }

    public void setTipo(TipoDistro tipo) {
        this.tipo = tipo;
    }

    public String getNome() {
        return nome;
    }

    public void setNome(String nome) {
        this.nome = nome;
    }

    public String getVersao() {
        return versao;
    }

    public void setVersao(String versao) {
        this.versao = versao;
    }

    public String getGerenciadorRede() {
        return gerenciadorRede;
    }

    public void setGerenciadorRede(String gerenciadorRede) {
        this.gerenciadorRede = gerenciadorRede;
    }

    public boolean isSuportada() {
        return tipo != TipoDistro.DESCONHECIDA;
    }
}

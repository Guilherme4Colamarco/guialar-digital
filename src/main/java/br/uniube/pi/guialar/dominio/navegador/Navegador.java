package br.uniube.pi.guialar.dominio.navegador;

/**
 * Representa um navegador detectado no sistema.
 */
public class Navegador {
    private String nome;
    private TipoNavegador tipo;
    private String caminhoExecutavel;
    private String caminhoPerfil;
    private boolean extensaoInstalada;

    public Navegador(String nome, TipoNavegador tipo, String caminhoExecutavel, 
                     String caminhoPerfil, boolean extensaoInstalada) {
        this.nome = nome;
        this.tipo = tipo;
        this.caminhoExecutavel = caminhoExecutavel;
        this.caminhoPerfil = caminhoPerfil;
        this.extensaoInstalada = extensaoInstalada;
    }

    public String getNome() {
        return nome;
    }

    public TipoNavegador getTipo() {
        return tipo;
    }

    public String getCaminhoExecutavel() {
        return caminhoExecutavel;
    }

    public String getCaminhoPerfil() {
        return caminhoPerfil;
    }

    public boolean isExtensaoInstalada() {
        return extensaoInstalada;
    }

    public void setExtensaoInstalada(boolean extensaoInstalada) {
        this.extensaoInstalada = extensaoInstalada;
    }

    public boolean isSuportado() {
        return tipo != TipoNavegador.DESCONHECIDO;
    }
}

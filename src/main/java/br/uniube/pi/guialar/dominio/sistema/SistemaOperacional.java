package br.uniube.pi.guialar.dominio.sistema;

/**
 * Interface base para detecção de sistema operacional.
 * 
 * Arquitetura: Core comum + adaptadores por OS.
 * 
 * MVP: Apenas Linux (Debian/Fedora/Arch) implementado.
 * Futuro: Mac, Windows, Android (apenas interfaces definidas).
 */
public interface SistemaOperacional {
    
    /**
     * Retorna o tipo do sistema operacional.
     */
    TipoSistema getTipo();
    
    /**
     * Retorna o nome amigável do sistema.
     */
    String getNome();
    
    /**
     * Retorna a versão do sistema.
     */
    String getVersao();
    
    /**
     * Verifica se o sistema é suportado pelo MVP.
     */
    boolean isSuportado();
}

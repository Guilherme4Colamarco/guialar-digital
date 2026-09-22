package br.uniube.pi.guialar.dominio.adaptadores;

import br.uniube.pi.guialar.dominio.navegador.Navegador;

import java.util.List;

/**
 * Interface para detectores de navegadores por sistema operacional.
 * 
 * Arquitetura limpa: Core comum + implementações específicas por OS.
 * 
 * MVP: Apenas LinuxBrowserDetector implementado.
 * 
 * Diferenças por OS:
 * 
 * Linux:
 * - PATH: /usr/bin/, /usr/local/bin/
 * - Perfis: ~/.mozilla/firefox/, ~/.config/chromium/
 * - .desktop files: /usr/share/applications/, ~/.local/share/applications/
 * 
 * macOS (futuro):
 * - Aplicações: /Applications/*.app/
 * - Homebrew: /usr/local/Caskroom/, /opt/homebrew/Caskroom/
 * - Perfis: ~/Library/Application Support/Firefox/, ~/Library/Application Support/Google/Chrome/
 * 
 * Windows (futuro):
 * - Program Files: C:\Program Files\, C:\Program Files (x86)\
 * - AppData: %LOCALAPPDATA%\, %APPDATA%\
 * - Registry: HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\
 * 
 * Android (futuro):
 * - Packages: pm list packages
 * - Limitações: sem acesso a perfis sem root
 */
public interface BrowserDetector {
    
    /**
     * Detecta todos os navegadores instalados no sistema.
     * 
     * @return Lista de navegadores detectados
     */
    List<Navegador> detectar();
    
    /**
     * Verifica se um navegador específico está instalado.
     * 
     * @param nome Nome do navegador (ex: "firefox", "chrome")
     * @return true se o navegador está instalado
     */
    boolean isInstalado(String nome);
}

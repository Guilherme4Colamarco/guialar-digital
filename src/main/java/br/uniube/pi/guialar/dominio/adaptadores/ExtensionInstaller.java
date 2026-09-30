package br.uniube.pi.guialar.dominio.adaptadores;

import br.uniube.pi.guialar.dominio.navegador.Navegador;

/**
 * Interface para instaladores de extensões por sistema operacional e navegador.
 * 
 * Arquitetura limpa: Core comum + implementações específicas.
 * 
 * MVP: Apenas LinuxExtensionInstaller implementado.
 * 
 * Diferenças por OS:
 * 
 * Linux:
 * - Firefox: policies.json em distribution/
 * - Chromium: managed policies (limitado, geralmente manual)
 * 
 * macOS (futuro):
 * - Firefox: /Applications/Firefox.app/Contents/Resources/distribution/policies.json
 * - Chrome: ~/Library/Application Support/Google/Chrome/External Extensions/
 * 
 * Windows (futuro):
 * - Firefox: policies.json em %PROGRAMFILES%\Mozilla Firefox\distribution\
 * - Chrome: Registry HKLM\SOFTWARE\Policies\Google\Chrome\ExtensionInstallForcelist
 * 
 * Android (futuro):
 * - Limitações severas: instalação manual via store
 * - Firefox: addons.mozilla.org mobile
 * - Chrome/Kiwi: chrome web store
 */
public interface ExtensionInstaller {
    
    /**
     * Instala a extensão apropriada no navegador.
     * 
     * @param navegador Navegador onde instalar a extensão
     * @return true se a instalação foi bem-sucedida
     */
    boolean instalar(Navegador navegador);
    
    /**
     * Verifica se a extensão está instalada.
     * 
     * @param navegador Navegador a verificar
     * @return true se a extensão está instalada
     */
    boolean isInstalada(Navegador navegador);
    
    /**
     * Remove a extensão do navegador.
     * 
     * @param navegador Navegador de onde remover
     * @return true se a remoção foi bem-sucedida
     */
    boolean remover(Navegador navegador);
}

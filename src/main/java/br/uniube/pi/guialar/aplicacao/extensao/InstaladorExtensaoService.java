package br.uniube.pi.guialar.aplicacao.extensao;

import br.uniube.pi.guialar.dominio.navegador.ExtensaoAdblocker;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Serviço para instalação de extensões de adblocker.
 * 
 * Firefox: uBlock Origin
 * Chromium: uBlock Origin Lite
 */
public class InstaladorExtensaoService {

    /**
     * Instala a extensão apropriada para o navegador.
     */
    public boolean instalar(Navegador navegador) {
        if (!navegador.isSuportado()) {
            return false;
        }

        ExtensaoAdblocker extensao = ExtensaoAdblocker.paraNavegador(navegador.getTipo());
        if (extensao == null) {
            return false;
        }

        try {
            if (navegador.getTipo() == TipoNavegador.FIREFOX) {
                return instalarFirefox(navegador, extensao);
            } else if (navegador.getTipo() == TipoNavegador.CHROMIUM) {
                return instalarChromium(navegador, extensao);
            }
        } catch (Exception e) {
            System.err.println("Erro ao instalar extensão: " + e.getMessage());
        }

        return false;
    }

    /**
     * Instala uBlock Origin no Firefox via arquivo de políticas.
     * Método recomendado: distribution/policies.json
     */
    private boolean instalarFirefox(Navegador navegador, ExtensaoAdblocker extensao) 
            throws IOException {
        
        Path perfilBase = Path.of(navegador.getCaminhoPerfil());
        if (!Files.exists(perfilBase)) {
            Files.createDirectories(perfilBase);
        }

        Path distributionDir = perfilBase.resolve("distribution");
        Files.createDirectories(distributionDir);

        Path policiesJson = distributionDir.resolve("policies.json");

        String politicas = String.format("""
            {
              "policies": {
                "Extensions": {
                  "Install": [
                    "https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi"
                  ]
                },
                "ExtensionSettings": {
                  "%s": {
                    "installation_mode": "force_installed",
                    "install_url": "%s"
                  }
                }
              }
            }
            """, extensao.getId(), extensao.getUrlDownload());

        Files.writeString(policiesJson, politicas);
        System.out.println("✓ uBlock Origin configurado para " + navegador.getNome());
        System.out.println("  Arquivo: " + policiesJson);
        System.out.println("  A extensão será instalada na próxima inicialização do navegador.");
        
        return true;
    }

    /**
     * Instala uBlock Origin Lite no Chromium via preferences.
     * Nota: Chromium requer que o usuário aceite a instalação manualmente
     * devido às políticas de segurança. Preparamos o arquivo mas não forçamos.
     */
    private boolean instalarChromium(Navegador navegador, ExtensaoAdblocker extensao) 
            throws IOException {
        
        Path perfilBase = Path.of(navegador.getCaminhoPerfil());
        if (!Files.exists(perfilBase)) {
            System.out.println("⚠ Perfil não encontrado: " + perfilBase);
            System.out.println("  Para Chromium, recomendamos instalação manual:");
            System.out.println("  1. Abra " + navegador.getNome());
            System.out.println("  2. Acesse chrome://extensions/ ou edge://extensions/");
            System.out.println("  3. Busque por 'uBlock Origin Lite' na Chrome Web Store");
            System.out.println("  4. Clique em 'Adicionar ao navegador'");
            return false;
        }

        Path extensionsInfoFile = perfilBase.resolve("extensions_to_install.txt");
        String info = String.format("""
            # Extensão recomendada pelo GuiaLar Digital
            # Nome: %s
            # ID: %s
            #
            # INSTALAÇÃO MANUAL NECESSÁRIA:
            # 1. Abra %s
            # 2. Acesse chrome://extensions/ (ou edge://extensions/)
            # 3. Busque por 'uBlock Origin Lite'
            # 4. Clique em 'Adicionar ao navegador'
            #
            # Link direto: https://chrome.google.com/webstore/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh
            """, extensao.getNome(), extensao.getId(), navegador.getNome());

        Files.writeString(extensionsInfoFile, info);
        System.out.println("⚠ " + navegador.getNome() + " requer instalação manual de extensões");
        System.out.println("  Instruções salvas em: " + extensionsInfoFile);
        System.out.println("  Extensão: " + extensao.getNome());
        
        return false;
    }
}

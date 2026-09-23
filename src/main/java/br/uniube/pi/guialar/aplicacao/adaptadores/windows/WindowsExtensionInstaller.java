package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.dominio.adaptadores.ExtensionInstaller;
import br.uniube.pi.guialar.dominio.navegador.ExtensaoAdblocker;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.navegador.TipoNavegador;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Preparação do bloqueador de anúncios no Windows.
 *
 * - Firefox: escreve {@code distribution/policies.json} na pasta de instalação
 *   (força a instalação do uBlock Origin na próxima abertura). Requer permissão
 *   de administrador para gravar em Program Files.
 * - Chromium/Edge e demais: deixa instruções guiadas em linguagem simples
 *   (instalação pela loja de extensões), como no Chromium do Linux.
 */
public class WindowsExtensionInstaller implements ExtensionInstaller {

    @Override
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
            }
            return instalarChromium(navegador, extensao);
        } catch (Exception e) {
            System.out.println("Não foi possível preparar o bloqueador em " + navegador.getNome()
                + " automaticamente: " + e.getMessage());
            imprimirInstrucoesManuais(navegador);
            return false;
        }
    }

    private boolean instalarFirefox(Navegador navegador, ExtensaoAdblocker extensao) throws IOException {
        Path executavel = Path.of(navegador.getCaminhoExecutavel());
        Path instalacao = executavel.getParent();
        if (instalacao == null) {
            imprimirInstrucoesManuais(navegador);
            return false;
        }

        Path distribution = instalacao.resolve("distribution");
        Files.createDirectories(distribution);
        Path policies = distribution.resolve("policies.json");

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

        Files.writeString(policies, politicas);
        System.out.println("✓ uBlock Origin configurado para " + navegador.getNome());
        System.out.println("  Arquivo: " + policies);
        System.out.println("  A extensão será instalada na próxima abertura do navegador.");
        return true;
    }

    private boolean instalarChromium(Navegador navegador, ExtensaoAdblocker extensao) throws IOException {
        Path perfil = Path.of(navegador.getCaminhoPerfil());
        if (Files.exists(perfil)) {
            Path info = perfil.resolve("extensions_to_install.txt");
            String conteudo = String.format("""
                # Recomendado pelo GuiaLar Digital
                # Bloqueador de anúncios: %s
                #
                # Como instalar:
                # 1. Abra o %s
                # 2. Acesse a loja de extensões (chrome://extensions ou edge://extensions)
                # 3. Procure por "%s" e clique em Adicionar
                """, extensao.getNome(), navegador.getNome(), extensao.getNome());
            try {
                Files.writeString(info, conteudo);
                System.out.println("  Instruções salvas em: " + info);
            } catch (Exception ignored) {
                // Sem permissão de escrita no perfil: segue apenas com instruções na tela.
            }
        }
        imprimirInstrucoesManuais(navegador);
        return false;
    }

    private void imprimirInstrucoesManuais(Navegador navegador) {
        System.out.println("⚠ " + navegador.getNome()
            + " requer instalar o bloqueador de anúncios pela loja de extensões.");
        System.out.println("  Passo a passo: abra o navegador → loja de extensões → procure 'uBlock Origin Lite' → Adicionar.");
    }

    @Override
    public boolean isInstalada(Navegador navegador) {
        return false;
    }

    @Override
    public boolean remover(Navegador navegador) {
        return false;
    }
}

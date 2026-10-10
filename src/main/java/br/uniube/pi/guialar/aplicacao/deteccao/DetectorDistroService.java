package br.uniube.pi.guialar.aplicacao.deteccao;

import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Serviço para detecção de distribuição Linux.
 * Suporta: Debian, Fedora e Arch Linux.
 */
public class DetectorDistroService {

    /**
     * Detecta a distribuição Linux atual.
     * Estratégia: lê /etc/os-release e arquivos específicos de cada distro.
     */
    public InfoDistro detectar() {
        try {
            TipoDistro tipo = detectarTipo();
            String nome = detectarNome(tipo);
            String versao = detectarVersao(tipo);
            String gerenciadorRede = detectarGerenciadorRede(tipo);

            return new InfoDistro(tipo, nome, versao, gerenciadorRede);
        } catch (Exception e) {
            return new InfoDistro(TipoDistro.DESCONHECIDA, "Desconhecida", "", "");
        }
    }

    private TipoDistro detectarTipo() throws IOException {
        Path osRelease = Path.of("/etc/os-release");
        
        if (!Files.exists(osRelease)) {
            return TipoDistro.DESCONHECIDA;
        }

        List<String> linhas = Files.readAllLines(osRelease);
        String conteudo = String.join("\n", linhas).toLowerCase();

        for (String linha : linhas) {
            if (linha.startsWith("ID=")) {
                String id = linha.substring(3).replaceAll("\"", "").trim().toLowerCase();
                if ("nixos".equals(id)) {
                    return TipoDistro.NIXOS;
                }
            }
        }

        if (conteudo.contains("debian") || conteudo.contains("ubuntu")) {
            return TipoDistro.DEBIAN;
        } else if (conteudo.contains("fedora")) {
            return TipoDistro.FEDORA;
        } else if (conteudo.contains("arch")) {
            return TipoDistro.ARCH;
        }

        return TipoDistro.DESCONHECIDA;
    }

    private String detectarNome(TipoDistro tipo) {
        try {
            Path osRelease = Path.of("/etc/os-release");
            List<String> linhas = Files.readAllLines(osRelease);
            
            for (String linha : linhas) {
                if (linha.startsWith("PRETTY_NAME=")) {
                    return linha.substring(12).replaceAll("\"", "");
                }
            }
        } catch (IOException e) {
            // Ignora erro
        }
        return tipo.getNomeExibicao();
    }

    private String detectarVersao(TipoDistro tipo) {
        try {
            Path osRelease = Path.of("/etc/os-release");
            List<String> linhas = Files.readAllLines(osRelease);
            
            for (String linha : linhas) {
                if (linha.startsWith("VERSION_ID=")) {
                    return linha.substring(11).replaceAll("\"", "");
                }
            }
        } catch (IOException e) {
            // Ignora erro
        }
        return "";
    }

    private String detectarGerenciadorRede(TipoDistro tipo) {
        if (comandoExiste("nmcli") && networkManagerRodando()) {
            return "NetworkManager";
        } else if (comandoExiste("systemctl") && servicoAtivo("systemd-resolved")) {
            return "systemd-resolved";
        } else if (comandoExiste("resolvconf")) {
            return "resolvconf";
        }
        return "manual";
    }

    private boolean comandoExiste(String comando) {
        try {
            Process process = new ProcessBuilder("which", comando)
                .redirectErrorStream(true)
                .start();
            int exitCode = process.waitFor();
            return exitCode == 0;
        } catch (Exception e) {
            return false;
        }
    }

    private boolean networkManagerRodando() {
        if (servicoAtivo("NetworkManager")) {
            return true;
        }
        try {
            Process process = new ProcessBuilder("nmcli", "-t", "-f", "RUNNING", "general")
                .redirectErrorStream(true)
                .start();
            try (var reader = new java.io.BufferedReader(
                    new java.io.InputStreamReader(process.getInputStream()))) {
                String linha = reader.readLine();
                return linha != null && linha.trim().equalsIgnoreCase("running");
            }
        } catch (Exception e) {
            return false;
        }
    }

    private boolean servicoAtivo(String servico) {
        try {
            Process process = new ProcessBuilder("systemctl", "is-active", servico)
                .redirectErrorStream(true)
                .start();
            int exitCode = process.waitFor();
            return exitCode == 0;
        } catch (Exception e) {
            return false;
        }
    }
}

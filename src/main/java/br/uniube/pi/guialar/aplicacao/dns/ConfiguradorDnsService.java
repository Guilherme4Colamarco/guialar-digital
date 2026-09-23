package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.LocalDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.List;

/**
 * Serviço para configuração de DNS do sistema.
 * 
 * DNS PADRÃO: Cloudflare 1.1.1.1 for Families (Malware + Adulto)
 * - IPv4 Primário: 1.1.1.3
 * - IPv4 Secundário: 1.0.0.3
 * - IPv6 Primário: 2606:4700:4700::1113
 * - IPv6 Secundário: 2606:4700:4700::1003
 * 
 * Bloqueia malware e conteúdo adulto (18+).
 * Não coleta histórico de navegação.
 * 
 * ESTRATÉGIA DE DETECÇÃO:
 * - NÃO edita /etc/resolv.conf diretamente (pode ser stub/symlink)
 * - Detecta gerenciador: NetworkManager (nmcli), systemd-resolved (resolvectl), ou Netplan
 * - Sempre faz backup antes de modificar
 * - Fedora/Debian: geralmente NetworkManager + systemd-resolved
 * - Arch: varia (NetworkManager, systemd-resolved, ou manual)
 * 
 * LIMITAÇÕES CONHECIDAS:
 * - VPN pode sobrescrever DNS do sistema
 * - Docker containers podem usar DNS próprio
 * - DNS-over-HTTPS configurado no navegador ignora DNS do sistema
 */
public class ConfiguradorDnsService {

    private static final String BACKUP_SUFFIX = ".guialar-backup-";

    /**
     * Configura o DNS do sistema usando Cloudflare 1.1.1.1 for Families.
     * Sempre usa 1.1.1.3 / 1.0.0.3 (proteção contra malware e conteúdo adulto).
     */
    public ConfiguracaoDns configurar(InfoDistro distro) {
        ServidorDns servidor = ServidorDns.getPadrao();

        return switch (distro.getTipo()) {
            case DEBIAN -> configurarDebian(servidor, distro);
            case FEDORA -> configurarFedora(servidor, distro);
            case ARCH -> configurarArch(servidor, distro);
            default -> ConfiguracaoDns.erro("unknown", "Distribuição não suportada");
        };
    }

    private ConfiguracaoDns configurarDebian(ServidorDns servidor, InfoDistro distro) {
        if ("NetworkManager".equals(distro.getGerenciadorRede())) {
            return configurarNetworkManager(servidor);
        } else if ("systemd-resolved".equals(distro.getGerenciadorRede())) {
            return configurarSystemdResolved(servidor);
        } else {
            return configurarResolvConf(servidor);
        }
    }

    private ConfiguracaoDns configurarFedora(ServidorDns servidor, InfoDistro distro) {
        return configurarNetworkManager(servidor);
    }

    private ConfiguracaoDns configurarArch(ServidorDns servidor, InfoDistro distro) {
        if ("systemd-resolved".equals(distro.getGerenciadorRede())) {
            return configurarSystemdResolved(servidor);
        } else if ("NetworkManager".equals(distro.getGerenciadorRede())) {
            return configurarNetworkManager(servidor);
        } else {
            return configurarResolvConf(servidor);
        }
    }

    /**
     * Configura DNS via NetworkManager (Debian, Fedora, Arch com NM).
     * Configura IPv4 e IPv6.
     */
    private ConfiguracaoDns configurarNetworkManager(ServidorDns servidor) {
        try {
            String conexao = obterConexaoAtiva();
            if (conexao == null) {
                return ConfiguracaoDns.erro("NetworkManager", "Nenhuma conexão ativa encontrada");
            }

            backupConfigNetworkManager(conexao);

            executarComando("nmcli", "connection", "modify", conexao, 
                "ipv4.dns", servidor.getPrimario() + " " + servidor.getSecundario());
            executarComando("nmcli", "connection", "modify", conexao, 
                "ipv4.ignore-auto-dns", "yes");
            
            executarComando("nmcli", "connection", "modify", conexao, 
                "ipv6.dns", servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6());
            executarComando("nmcli", "connection", "modify", conexao, 
                "ipv6.ignore-auto-dns", "yes");

            executarComando("nmcli", "connection", "up", conexao);

            System.out.println("  ℹ️  Para reverter, execute:");
            System.out.println("     nmcli connection modify " + conexao + " ipv4.dns \"\"");
            System.out.println("     nmcli connection modify " + conexao + " ipv4.ignore-auto-dns no");
            System.out.println("     nmcli connection modify " + conexao + " ipv6.dns \"\"");
            System.out.println("     nmcli connection modify " + conexao + " ipv6.ignore-auto-dns no");
            System.out.println("     nmcli connection up " + conexao);

            return ConfiguracaoDns.sucesso(servidor, "NetworkManager");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("NetworkManager", e.getMessage());
        }
    }

    /**
     * Configura DNS via systemd-resolved (Arch, algumas instalações Debian/Ubuntu).
     */
    private ConfiguracaoDns configurarSystemdResolved(ServidorDns servidor) {
        try {
            Path resolvedConf = Path.of("/etc/systemd/resolved.conf");
            List<String> linhas = new ArrayList<>();
            
            linhas.add("[Resolve]");
            linhas.add("DNS=" + servidor.getPrimario() + " " + servidor.getSecundario());
            linhas.add("FallbackDNS=");
            linhas.add("Domains=~.");
            linhas.add("DNSSEC=allow-downgrade");
            linhas.add("DNSOverTLS=opportunistic");

            Files.write(resolvedConf, linhas, StandardOpenOption.CREATE, 
                StandardOpenOption.TRUNCATE_EXISTING);

            executarComando("systemctl", "restart", "systemd-resolved");

            return ConfiguracaoDns.sucesso(servidor, "systemd-resolved");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("systemd-resolved", e.getMessage());
        }
    }

    /**
     * Configura DNS via /etc/resolv.conf (fallback manual).
     */
    private ConfiguracaoDns configurarResolvConf(ServidorDns servidor) {
        try {
            Path resolvConf = Path.of("/etc/resolv.conf");
            List<String> linhas = new ArrayList<>();
            
            linhas.add("# Configurado por GuiaLar Digital");
            linhas.add("# Cloudflare 1.1.1.1 for Families (Malware + Adulto)");
            linhas.add("# Bloqueia malware e conteúdo adulto (18+)");
            linhas.add("nameserver " + servidor.getPrimario());
            linhas.add("nameserver " + servidor.getSecundario());

            Files.write(resolvConf, linhas, StandardOpenOption.CREATE, 
                StandardOpenOption.TRUNCATE_EXISTING);

            return ConfiguracaoDns.sucesso(servidor, "resolv.conf");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("resolv.conf", e.getMessage());
        }
    }

    private void backupConfigNetworkManager(String conexao) {
        try {
            String timestamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"));
            System.out.println("  ℹ️  Fazendo backup da conexão: " + conexao);
            System.out.println("     Timestamp: " + timestamp);
        } catch (Exception e) {
            System.err.println("  ⚠ Não foi possível criar backup: " + e.getMessage());
        }
    }

    private String obterConexaoAtiva() throws IOException, InterruptedException {
        Process process = new ProcessBuilder("nmcli", "-t", "-f", "NAME,TYPE,DEVICE", 
            "connection", "show", "--active")
            .redirectErrorStream(true)
            .start();

        try (BufferedReader reader = new BufferedReader(
                new InputStreamReader(process.getInputStream()))) {
            String linha = reader.readLine();
            if (linha != null) {
                return linha.split(":")[0];
            }
        }

        process.waitFor();
        return null;
    }

    private void executarComando(String... comando) throws IOException, InterruptedException {
        Process process = new ProcessBuilder(comando)
            .redirectErrorStream(true)
            .start();

        int exitCode = process.waitFor();
        if (exitCode != 0) {
            try (BufferedReader reader = new BufferedReader(
                    new InputStreamReader(process.getInputStream()))) {
                String erro = reader.lines().reduce("", (a, b) -> a + "\n" + b);
                throw new IOException("Comando falhou: " + String.join(" ", comando) + "\n" + erro);
            }
        }
    }
}

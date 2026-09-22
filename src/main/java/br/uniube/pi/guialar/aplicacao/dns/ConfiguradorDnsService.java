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
     * USA DROP-IN para não truncar o arquivo original.
     */
    private ConfiguracaoDns configurarSystemdResolved(ServidorDns servidor) {
        try {
            // Usar drop-in ao invés de truncar o arquivo principal
            Path dropinDir = Path.of("/etc/systemd/resolved.conf.d");
            Path dropinFile = dropinDir.resolve("99-guialar.conf");
            
            // Criar diretório se não existir
            Files.createDirectories(dropinDir);
            
            // Fazer backup do drop-in se já existir
            if (Files.exists(dropinFile)) {
                String timestamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"));
                Path backup = Path.of("/etc/systemd/resolved.conf.d/99-guialar.conf" + BACKUP_SUFFIX + timestamp);
                Files.copy(dropinFile, backup, StandardCopyOption.REPLACE_EXISTING);
                System.out.println("  ℹ️  Backup do drop-in: " + backup);
            }
            
            // Criar drop-in com configuração Cloudflare Families
            List<String> linhas = new ArrayList<>();
            linhas.add("# Configurado por GuiaLar Digital");
            linhas.add("# Cloudflare 1.1.1.1 for Families (Malware + Adulto)");
            linhas.add("# Para reverter: sudo rm " + dropinFile + " && sudo systemctl restart systemd-resolved");
            linhas.add("");
            linhas.add("[Resolve]");
            linhas.add("DNS=" + servidor.getPrimario() + " " + servidor.getSecundario() + " " + 
                       servidor.getPrimarioIpv6() + " " + servidor.getSecundarioIpv6());
            linhas.add("FallbackDNS=");
            linhas.add("Domains=~.");
            linhas.add("DNSSEC=allow-downgrade");
            linhas.add("DNSOverTLS=opportunistic");

            Files.write(dropinFile, linhas, StandardOpenOption.CREATE, 
                StandardOpenOption.TRUNCATE_EXISTING);

            executarComando("systemctl", "restart", "systemd-resolved");

            System.out.println("  ℹ️  Para reverter:");
            System.out.println("     sudo rm " + dropinFile);
            System.out.println("     sudo systemctl restart systemd-resolved");

            return ConfiguracaoDns.sucesso(servidor, "systemd-resolved (drop-in)");
        } catch (Exception e) {
            return ConfiguracaoDns.erro("systemd-resolved", e.getMessage());
        }
    }

    /**
     * Configura DNS via /etc/resolv.conf (fallback manual).
     * VERIFICA se é symlink stub antes de editar.
     */
    private ConfiguracaoDns configurarResolvConf(ServidorDns servidor) {
        try {
            Path resolvConf = Path.of("/etc/resolv.conf");
            
            // Verificar se é symlink (geralmente aponta para stub do systemd)
            if (Files.isSymbolicLink(resolvConf)) {
                Path target = Files.readSymbolicLink(resolvConf);
                System.out.println("  ⚠ /etc/resolv.conf é um symlink para: " + target);
                
                if (target.toString().contains("systemd") || target.toString().contains("stub")) {
                    return ConfiguracaoDns.erro("resolv.conf", 
                        "É um stub do systemd. Use systemd-resolved ou NetworkManager.");
                }
            }
            
            // Fazer backup
            if (Files.exists(resolvConf)) {
                String timestamp = LocalDateTime.now().format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss"));
                Path backup = Path.of("/etc/resolv.conf" + BACKUP_SUFFIX + timestamp);
                Files.copy(resolvConf, backup, StandardCopyOption.REPLACE_EXISTING);
                System.out.println("  ℹ️  Backup: " + backup);
            }
            
            // Criar novo resolv.conf com IPv4 E IPv6
            List<String> linhas = new ArrayList<>();
            linhas.add("# Configurado por GuiaLar Digital");
            linhas.add("# Cloudflare 1.1.1.1 for Families (Malware + Adulto)");
            linhas.add("# Bloqueia malware e conteúdo adulto (18+)");
            linhas.add("");
            linhas.add("# IPv4");
            linhas.add("nameserver " + servidor.getPrimario());
            linhas.add("nameserver " + servidor.getSecundario());
            linhas.add("");
            linhas.add("# IPv6");
            linhas.add("nameserver " + servidor.getPrimarioIpv6());
            linhas.add("nameserver " + servidor.getSecundarioIpv6());

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
            
            // Obter configuração DNS atual
            Process processDns = new ProcessBuilder("nmcli", "-t", "-f", "ipv4.dns,ipv6.dns", 
                "connection", "show", conexao)
                .redirectErrorStream(true)
                .start();
            
            BufferedReader reader = new BufferedReader(new InputStreamReader(processDns.getInputStream()));
            List<String> config = new ArrayList<>();
            String linha;
            
            while ((linha = reader.readLine()) != null) {
                config.add(linha);
            }
            
            processDns.waitFor();
            
            // Salvar backup em arquivo
            Path backupDir = Path.of(System.getProperty("user.home"), ".guialar", "backups");
            Files.createDirectories(backupDir);
            
            Path backupFile = backupDir.resolve("nm-" + conexao + "-" + timestamp + ".txt");
            Files.write(backupFile, config);
            
            System.out.println("  ℹ️  Backup salvo: " + backupFile);
            System.out.println("     Conexão: " + conexao);
            
        } catch (Exception e) {
            System.err.println("  ⚠ Não foi possível criar backup: " + e.getMessage());
        }
    }

    private String obterConexaoAtiva() throws IOException, InterruptedException {
        Process process = new ProcessBuilder("nmcli", "-t", "-f", "NAME,TYPE,DEVICE", 
            "connection", "show", "--active")
            .redirectErrorStream(true)
            .start();

        String melhorConexao = null;
        
        // Tipos de conexão que devemos EVITAR (VPN, docker, bridges)
        String[] tiposIgnorar = {"vpn", "tun", "docker", "bridge", "veth"};
        
        // Tipos preferidos (em ordem de prioridade)
        String[] tiposPreferidos = {"802-3-ethernet", "ethernet", "802-11-wireless", "wifi"};
        
        try (BufferedReader reader = new BufferedReader(
                new InputStreamReader(process.getInputStream()))) {
            String linha;
            
            while ((linha = reader.readLine()) != null) {
                String[] partes = linha.split(":");
                if (partes.length < 3) continue;
                
                String nome = partes[0];
                String tipo = partes[1].toLowerCase();
                String device = partes[2];
                
                // Ignorar VPN, docker, bridges
                boolean ignorar = false;
                for (String tipoIgnorar : tiposIgnorar) {
                    if (tipo.contains(tipoIgnorar) || device.contains(tipoIgnorar)) {
                        ignorar = true;
                        break;
                    }
                }
                
                if (ignorar) {
                    continue;
                }
                
                // Priorizar ethernet/wifi
                for (String tipoPreferido : tiposPreferidos) {
                    if (tipo.contains(tipoPreferido)) {
                        melhorConexao = nome;
                        break;
                    }
                }
                
                // Se ainda não temos conexão, pegar esta
                if (melhorConexao == null) {
                    melhorConexao = nome;
                }
                
                // Se achamos ethernet, preferir ela e parar
                if (tipo.contains("ethernet") || tipo.contains("802-3")) {
                    break;
                }
            }
        }

        process.waitFor();
        return melhorConexao;
    }

    public void executarComando(String... comando) throws IOException, InterruptedException {
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

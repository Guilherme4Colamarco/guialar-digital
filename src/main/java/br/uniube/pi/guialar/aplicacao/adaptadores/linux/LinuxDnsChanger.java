package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.adaptadores.DnsChanger;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;

/**
 * Implementação Linux do DnsChanger.
 * 
 * MVP: ÚNICA IMPLEMENTAÇÃO FUNCIONAL.
 * 
 * Suporta:
 * - NetworkManager (nmcli)
 * - systemd-resolved (resolvectl)
 * - Netplan
 * - /etc/resolv.conf (fallback)
 * 
 * Distribuições: Debian, Ubuntu, Fedora, Arch Linux
 */
public class LinuxDnsChanger implements DnsChanger {
    
    private final InfoDistro distro;
    private final ConfiguradorDnsService configurador;
    
    public LinuxDnsChanger(InfoDistro distro) {
        this.distro = distro;
        this.configurador = new ConfiguradorDnsService();
    }

    @Override
    public ConfiguracaoDns configurar(ServidorDns servidor) {
        return configurador.configurar(distro);
    }

    @Override
    public boolean verificar() {
        try {
            // Verificar via resolvectl ou dig se está usando Cloudflare Families
            ProcessBuilder pb = new ProcessBuilder("resolvectl", "status");
            pb.redirectErrorStream(true);
            Process process = pb.start();
            
            java.io.BufferedReader reader = new java.io.BufferedReader(
                new java.io.InputStreamReader(process.getInputStream())
            );
            
            String linha;
            boolean encontrouFamilies = false;
            
            while ((linha = reader.readLine()) != null) {
                if (linha.contains("1.1.1.3") || linha.contains("1.0.0.3") ||
                    linha.contains("2606:4700:4700::1113") || linha.contains("2606:4700:4700::1003")) {
                    encontrouFamilies = true;
                    break;
                }
            }
            
            process.waitFor();
            return encontrouFamilies;
            
        } catch (Exception e) {
            // Se resolvectl não funcionar, tentar ler /etc/resolv.conf
            try {
                java.nio.file.Path resolvConf = java.nio.file.Path.of("/etc/resolv.conf");
                if (java.nio.file.Files.exists(resolvConf)) {
                    String conteudo = java.nio.file.Files.readString(resolvConf);
                    return conteudo.contains("1.1.1.3") || conteudo.contains("1.0.0.3");
                }
            } catch (Exception e2) {
                // Ignora
            }
            return false;
        }
    }

    @Override
    public ConfiguracaoDns reverter() {
        String gerenciador = distro.getGerenciadorRede();
        
        try {
            if ("NetworkManager".equals(gerenciador)) {
                return reverterNetworkManager();
            } else if ("systemd-resolved".equals(gerenciador)) {
                return reverterSystemdResolved();
            } else {
                return reverterResolvConf();
            }
        } catch (Exception e) {
            return ConfiguracaoDns.erro(gerenciador, 
                "Erro na reversão automática: " + e.getMessage() + ". Use as instruções manuais.");
        }
    }
    
    private ConfiguracaoDns reverterNetworkManager() throws Exception {
        // Procurar backup mais recente
        java.nio.file.Path backupDir = java.nio.file.Path.of(System.getProperty("user.home"), ".guialar", "backups");
        
        if (!java.nio.file.Files.exists(backupDir)) {
            throw new Exception("Diretório de backup não encontrado");
        }
        
        // Listar backups do NetworkManager
        java.util.List<java.nio.file.Path> backups = new java.util.ArrayList<>();
        try (java.nio.file.DirectoryStream<java.nio.file.Path> stream = java.nio.file.Files.newDirectoryStream(backupDir, "nm-*.txt")) {
            for (java.nio.file.Path entry : stream) {
                backups.add(entry);
            }
        }
        
        if (backups.isEmpty()) {
            throw new Exception("Nenhum backup encontrado");
        }
        
        // Pegar o mais recente
        backups.sort(java.util.Comparator.comparing(java.nio.file.Path::toString).reversed());
        java.nio.file.Path backup = backups.get(0);
        
        // Ler configuração do backup
        java.util.List<String> config = java.nio.file.Files.readAllLines(backup);
        
        // Extrair nome da conexão do nome do arquivo
        String filename = backup.getFileName().toString();
        String conexao = filename.substring(3, filename.lastIndexOf('-'));
        
        // Limpar DNS
        configurador.executarComando("nmcli", "connection", "modify", conexao, "ipv4.dns", "");
        configurador.executarComando("nmcli", "connection", "modify", conexao, "ipv4.ignore-auto-dns", "no");
        configurador.executarComando("nmcli", "connection", "modify", conexao, "ipv6.dns", "");
        configurador.executarComando("nmcli", "connection", "modify", conexao, "ipv6.ignore-auto-dns", "no");
        configurador.executarComando("nmcli", "connection", "up", conexao);
        
        return new ConfiguracaoDns(null, true, "NetworkManager", "DNS revertido para padrão");
    }
    
    private ConfiguracaoDns reverterSystemdResolved() throws Exception {
        java.nio.file.Path dropinFile = java.nio.file.Path.of("/etc/systemd/resolved.conf.d/99-guialar.conf");
        
        if (java.nio.file.Files.exists(dropinFile)) {
            java.nio.file.Files.delete(dropinFile);
            configurador.executarComando("systemctl", "restart", "systemd-resolved");
            return new ConfiguracaoDns(null, true, "systemd-resolved", "Drop-in removido, DNS revertido");
        } else {
            throw new Exception("Drop-in não encontrado");
        }
    }
    
    private ConfiguracaoDns reverterResolvConf() throws Exception {
        // Procurar backup mais recente
        java.nio.file.Path resolvConf = java.nio.file.Path.of("/etc/resolv.conf");
        String backupPattern = "/etc/resolv.conf.guialar-backup-*";
        
        // Listar backups
        java.nio.file.Path etcDir = java.nio.file.Path.of("/etc");
        java.util.List<java.nio.file.Path> backups = new java.util.ArrayList<>();
        
        try (java.nio.file.DirectoryStream<java.nio.file.Path> stream = java.nio.file.Files.newDirectoryStream(etcDir, "resolv.conf.guialar-backup-*")) {
            for (java.nio.file.Path entry : stream) {
                backups.add(entry);
            }
        }
        
        if (backups.isEmpty()) {
            throw new Exception("Nenhum backup encontrado");
        }
        
        // Pegar o mais recente
        backups.sort(java.util.Comparator.comparing(java.nio.file.Path::toString).reversed());
        java.nio.file.Path backup = backups.get(0);
        
        // Restaurar backup
        java.nio.file.Files.copy(backup, resolvConf, java.nio.file.StandardCopyOption.REPLACE_EXISTING);
        
        return new ConfiguracaoDns(null, true, "resolv.conf", "Backup restaurado: " + backup.getFileName());
    }

    @Override
    public String getInstrucoesReversao() {
        StringBuilder sb = new StringBuilder();
        sb.append("Para reverter o DNS no Linux:\n\n");
        
        switch (distro.getGerenciadorRede()) {
            case "NetworkManager":
                sb.append("NetworkManager:\n");
                sb.append("  nmcli connection show\n");
                sb.append("  nmcli connection modify \"SUA-CONEXÃO\" ipv4.dns \"\"\n");
                sb.append("  nmcli connection modify \"SUA-CONEXÃO\" ipv4.ignore-auto-dns no\n");
                sb.append("  nmcli connection modify \"SUA-CONEXÃO\" ipv6.dns \"\"\n");
                sb.append("  nmcli connection modify \"SUA-CONEXÃO\" ipv6.ignore-auto-dns no\n");
                sb.append("  nmcli connection up \"SUA-CONEXÃO\"\n");
                break;
                
            case "systemd-resolved":
                sb.append("systemd-resolved:\n");
                sb.append("  Edite /etc/systemd/resolved.conf\n");
                sb.append("  Remova ou comente as linhas DNS=\n");
                sb.append("  systemctl restart systemd-resolved\n");
                break;
                
            default:
                sb.append("/etc/resolv.conf:\n");
                sb.append("  Restaure o backup ou reconfigure manualmente\n");
                break;
        }
        
        return sb.toString();
    }
}

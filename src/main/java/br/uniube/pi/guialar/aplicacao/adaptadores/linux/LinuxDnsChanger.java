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
        // TODO: Implementar verificação se DNS está configurado
        return true;
    }

    @Override
    public ConfiguracaoDns reverter() {
        // TODO: Implementar reversão automática
        return ConfiguracaoDns.erro(distro.getGerenciadorRede(), 
            "Reversão automática não implementada. Use as instruções manuais.");
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

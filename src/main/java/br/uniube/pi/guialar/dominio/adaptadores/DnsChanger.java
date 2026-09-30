package br.uniube.pi.guialar.dominio.adaptadores;

import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

/**
 * Interface para adaptadores de troca de DNS por sistema operacional.
 * 
 * Arquitetura limpa: Core comum + implementações específicas por OS.
 * 
 * MVP: Apenas LinuxDnsChanger implementado.
 * 
 * Futuro (interfaces definidas, sem implementação):
 * - MacOsDnsChanger: networksetup/scutil (não NetworkManager/systemd-resolved)
 * - WindowsDnsChanger: netsh/PowerShell (Set-DnsClientServerAddress)
 * - AndroidDnsChanger: Private DNS over TLS (sem root, não muda DNS do sistema como desktop)
 */
public interface DnsChanger {
    
    /**
     * Configura o DNS do sistema para o servidor especificado.
     * 
     * @param servidor Servidor DNS a configurar (ex: Cloudflare Families)
     * @return Resultado da configuração
     */
    ConfiguracaoDns configurar(ServidorDns servidor);
    
    /**
     * Verifica se o DNS está configurado corretamente.
     * 
     * @return true se o DNS está configurado
     */
    boolean verificar();
    
    /**
     * Reverte a configuração DNS para o padrão do sistema.
     * 
     * @return Resultado da reversão
     */
    ConfiguracaoDns reverter();
    
    /**
     * Retorna instruções de como reverter manualmente (caso a reversão automática falhe).
     * 
     * @return Instruções de reversão manual
     */
    String getInstrucoesReversao();
}

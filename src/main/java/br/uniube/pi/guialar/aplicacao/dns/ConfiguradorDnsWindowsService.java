package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

/**
 * Serviço para configuração de DNS no Windows (ESBOÇO INICIAL).
 * 
 * Estratégia Windows:
 * - Usa PowerShell: Set-DnsClientServerAddress
 * - Configuração por adaptador de rede
 * 
 * NOTA: Esta é uma implementação mínima/esboço.
 * O MVP completo está focado em Linux.
 * 
 * Limitação Windows: DNS-over-HTTPS no Edge/Chrome é configurado
 * separadamente no navegador e pode ignorar o DNS do sistema.
 */
public class ConfiguradorDnsWindowsService {

    /**
     * Configura DNS no Windows (ESBOÇO - NÃO IMPLEMENTADO COMPLETAMENTE).
     */
    public ConfiguracaoDns configurar() {
        System.out.println("⚠️  Configuração de DNS no Windows - ESBOÇO");
        System.out.println();
        System.out.println("Para configurar DNS no Windows manualmente:");
        System.out.println();
        System.out.println("1. Abra PowerShell como Administrador");
        System.out.println();
        System.out.println("2. Liste os adaptadores de rede:");
        System.out.println("   Get-NetAdapter");
        System.out.println();
        System.out.println("3. Configure o DNS (substitua 'Ethernet' pelo seu adaptador):");
        
        ServidorDns servidor = ServidorDns.getPadrao();
        System.out.println("   Set-DnsClientServerAddress -InterfaceAlias \"Ethernet\" -ServerAddresses (\"" + 
            servidor.getPrimario() + "\",\"" + servidor.getSecundario() + "\")");
        System.out.println();
        System.out.println("4. Verifique a configuração:");
        System.out.println("   Get-DnsClientServerAddress");
        System.out.println();
        System.out.println("5. Teste o DNS:");
        System.out.println("   nslookup example.com " + servidor.getPrimario());
        System.out.println("   nslookup malware.testcategory.com " + servidor.getPrimario());
        System.out.println();
        System.out.println("IMPORTANTE:");
        System.out.println("- Edge e Chrome podem usar DNS-over-HTTPS próprio");
        System.out.println("- Configure DoH nos navegadores para usar Cloudflare Families:");
        System.out.println("  Edge: edge://settings/privacy → Usar DNS seguro");
        System.out.println("  Chrome: chrome://settings/security → Usar DNS seguro");
        System.out.println();

        return ConfiguracaoDns.erro("Windows", 
            "Implementação automática não disponível. Use as instruções manuais acima.");
    }
}

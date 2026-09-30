package br.uniube.pi.guialar.dominio.dns;

/**
 * Servidores DNS seguros disponíveis.
 * 
 * PADRÃO MVP: Cloudflare 1.1.1.1 for Families (Malware e Adulto)
 * - Bloqueia malware
 * - Bloqueia conteúdo adulto/18+
 * 
 * IPs IPv4:
 * - Primário: 1.1.1.3
 * - Secundário: 1.0.0.3
 * 
 * IPs IPv6:
 * - Primário: 2606:4700:4700::1113
 * - Secundário: 2606:4700:4700::1003
 * 
 * DNS-over-HTTPS (DoH) endpoint para navegadores:
 * - URL: https://family.cloudflare-dns.com/dns-query
 * - IMPORTANTE: NÃO use o DoH genérico (dns.cloudflare.com) - não bloqueia!
 * 
 * URLs de teste:
 * - malware.testcategory.com (deve retornar 0.0.0.0 se bloqueado)
 * - nudity.testcategory.com (deve retornar 0.0.0.0 se bloqueado)
 * 
 * Documentação: https://developers.cloudflare.com/1.1.1.1/setup/
 * 
 * LIMITAÇÕES CONHECIDAS:
 * - VPN: Conexões VPN podem sobrescrever o DNS do sistema
 * - Docker: Containers podem usar DNS próprio
 * - DoH no navegador: DNS-over-HTTPS configurado no navegador IGNORA COMPLETAMENTE
 *   o DNS do sistema. Configure o navegador para usar family.cloudflare-dns.com
 */
public enum ServidorDns {
    /**
     * Cloudflare 1.1.1.1 for Families - Bloqueia Malware e Conteúdo Adulto
     * Este é o DNS padrão do GuiaLar Digital MVP.
     * 
     * Proteção:
     * - Bloqueia sites de malware
     * - Bloqueia sites adultos (18+)
     * - Não coleta histórico de navegação
     */
    CLOUDFLARE_FAMILIES_MALWARE_ADULT(
        "Cloudflare for Families (Malware + Adulto)",
        "1.1.1.3",
        "1.0.0.3",
        "2606:4700:4700::1113",
        "2606:4700:4700::1003",
        "Bloqueia malware e conteúdo adulto (18+)"
    );

    private final String nome;
    private final String primario;
    private final String secundario;
    private final String primarioIpv6;
    private final String secundarioIpv6;
    private final String descricao;
    
    // Constantes de teste e configuração
    public static final String DOH_ENDPOINT = "https://family.cloudflare-dns.com/dns-query";
    public static final String URL_TESTE_MALWARE = "malware.testcategory.com";
    public static final String URL_TESTE_NUDITY = "nudity.testcategory.com";

    ServidorDns(String nome, String primario, String secundario, 
                String primarioIpv6, String secundarioIpv6, String descricao) {
        this.nome = nome;
        this.primario = primario;
        this.secundario = secundario;
        this.primarioIpv6 = primarioIpv6;
        this.secundarioIpv6 = secundarioIpv6;
        this.descricao = descricao;
    }

    public String getNome() {
        return nome;
    }

    public String getPrimario() {
        return primario;
    }

    public String getSecundario() {
        return secundario;
    }

    public String getPrimarioIpv6() {
        return primarioIpv6;
    }

    public String getSecundarioIpv6() {
        return secundarioIpv6;
    }

    public String getDescricao() {
        return descricao;
    }

    /**
     * Retorna o servidor DNS padrão do GuiaLar Digital.
     * MVP: Sempre Cloudflare 1.1.1.3 / 1.0.0.3 (Families Malware + Adulto)
     */
    public static ServidorDns getPadrao() {
        return CLOUDFLARE_FAMILIES_MALWARE_ADULT;
    }
}

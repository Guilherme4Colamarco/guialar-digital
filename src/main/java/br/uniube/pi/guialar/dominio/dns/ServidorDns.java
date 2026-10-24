package br.uniube.pi.guialar.dominio.dns;

import java.util.Arrays;
import java.util.Locale;

/**
 * Opções de DNS do GuiaLar Digital.
 *
 * <ul>
 *   <li>{@link #CLOUDFLARE_FAMILIES_MALWARE_ADULT} — IPs públicos no DNS do sistema
 *       (malware + adulto), sem conta.</li>
 *   <li>{@link #UBLOCKDNS} — uBlockDNS (ublockdns.com): bloqueio de anúncios/rastreadores
 *       no DNS para <b>todos os apps</b> via cliente local (Linux/Windows). Exige perfil
 *       na dashboard. Projeto independente — não é o uBlock Origin.</li>
 * </ul>
 */
public enum ServidorDns {
    CLOUDFLARE_FAMILIES_MALWARE_ADULT(
        "families",
        "Cloudflare for Families (Malware + Adulto)",
        "1.1.1.3",
        "1.0.0.3",
        "2606:4700:4700::1113",
        "2606:4700:4700::1003",
        "Bloqueia malware e conteúdo adulto (18+) em todos os apps via DNS do sistema",
        "https://family.cloudflare-dns.com/dns-query",
        new String[]{"malware.testcategory.com", "nudity.testcategory.com"},
        false,
        false
    ),

    /**
     * uBlockDNS: cliente local + DoH com perfil. Não use estes IPs sem o serviço instalado.
     */
    UBLOCKDNS(
        "ublockdns",
        "uBlockDNS (todos os apps neste computador)",
        "127.0.0.1",
        "127.0.0.1",
        "::1",
        "::1",
        "Anúncios/rastreadores no DNS do sistema (cliente ublockdns.com — Linux e Windows)",
        "https://ublockdns.com",
        new String[]{"ads.adtidy.org", "doubleclick.net"},
        true,
        true
    );

    private final String idCli;
    private final String nome;
    private final String primario;
    private final String secundario;
    private final String primarioIpv6;
    private final String secundarioIpv6;
    private final String descricao;
    private final String dohEndpoint;
    private final String[] dominiosTesteBloqueio;
    private final boolean nxdomainComoBloqueio;
    private final boolean requerClienteExterno;

    /** @deprecated Prefer {@link #getDohEndpoint()} por servidor. */
    public static final String DOH_ENDPOINT = CLOUDFLARE_FAMILIES_MALWARE_ADULT.dohEndpoint;
    public static final String URL_TESTE_MALWARE = "malware.testcategory.com";
    public static final String URL_TESTE_NUDITY = "nudity.testcategory.com";
    public static final String UBLOCKDNS_SITE = "https://ublockdns.com";
    public static final String UBLOCKDNS_SETUP = "https://ublockdns.com/";

    ServidorDns(String idCli, String nome, String primario, String secundario,
                String primarioIpv6, String secundarioIpv6, String descricao,
                String dohEndpoint, String[] dominiosTesteBloqueio,
                boolean nxdomainComoBloqueio, boolean requerClienteExterno) {
        this.idCli = idCli;
        this.nome = nome;
        this.primario = primario;
        this.secundario = secundario;
        this.primarioIpv6 = primarioIpv6;
        this.secundarioIpv6 = secundarioIpv6;
        this.descricao = descricao;
        this.dohEndpoint = dohEndpoint;
        this.dominiosTesteBloqueio = dominiosTesteBloqueio;
        this.nxdomainComoBloqueio = nxdomainComoBloqueio;
        this.requerClienteExterno = requerClienteExterno;
    }

    public String getIdCli() { return idCli; }
    public String getNome() { return nome; }
    public String getPrimario() { return primario; }
    public String getSecundario() { return secundario; }
    public String getPrimarioIpv6() { return primarioIpv6; }
    public String getSecundarioIpv6() { return secundarioIpv6; }
    public String getDescricao() { return descricao; }
    public String getDohEndpoint() { return dohEndpoint; }

    public String[] getDominiosTesteBloqueio() {
        return Arrays.copyOf(dominiosTesteBloqueio, dominiosTesteBloqueio.length);
    }

    public boolean isNxdomainComoBloqueio() { return nxdomainComoBloqueio; }

    /** True = não aplicar IPs direto; usar assistente de instalação do cliente. */
    public boolean isRequerClienteExterno() { return requerClienteExterno; }

    public boolean apareceEm(String texto) {
        if (texto == null || texto.isBlank() || this == UBLOCKDNS) {
            return false;
        }
        return texto.contains(primario)
            || texto.contains(secundario)
            || texto.contains(primarioIpv6)
            || texto.contains(secundarioIpv6);
    }

    public static ServidorDns getPadrao() {
        return CLOUDFLARE_FAMILIES_MALWARE_ADULT;
    }

    /**
     * {@code --dns=families|ublockdns|ublock|adguard}.
     * {@code adguard} permanece como alias legado → uBlockDNS.
     */
    public static ServidorDns deArgumento(String valor) {
        if (valor == null || valor.isBlank()) {
            return getPadrao();
        }
        String v = valor.trim().toLowerCase(Locale.ROOT);
        return switch (v) {
            case "families", "family", "cloudflare", "cf" -> CLOUDFLARE_FAMILIES_MALWARE_ADULT;
            case "ublockdns", "ublock", "ublock-dns", "adguard", "ads", "anuncios", "anúncios"
                -> UBLOCKDNS;
            default -> throw new IllegalArgumentException(
                "DNS desconhecido: " + valor + " (use families ou ublockdns)");
        };
    }

    public static ServidorDns detectarNoTexto(String texto) {
        if (texto == null || texto.isBlank()) {
            return null;
        }
        if (CLOUDFLARE_FAMILIES_MALWARE_ADULT.apareceEm(texto)) {
            return CLOUDFLARE_FAMILIES_MALWARE_ADULT;
        }
        return null;
    }

    public static boolean textoContemQualquerAlvo(String texto) {
        return detectarNoTexto(texto) != null;
    }
}

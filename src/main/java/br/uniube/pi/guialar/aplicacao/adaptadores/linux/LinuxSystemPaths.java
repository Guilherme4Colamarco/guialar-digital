package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Caminhos de sistema fixos usados em operações elevadas (nunca lidos do manifesto).
 */
public final class LinuxSystemPaths {

    public static final String DROPIN_RESOLVED = "/etc/systemd/resolved.conf.d/99-guialar.conf";
    public static final String DIR_RESOLVED_DROPIN = "/etc/systemd/resolved.conf.d";
    public static final String NETPLAN_GUAILAR = "/etc/netplan/99-guialar-dns.yaml";
    public static final String RESOLV_CONF = "/etc/resolv.conf";
    public static final String RESOLV_GUAILAR_BACKUP = "/etc/resolv.conf.guialar-backup";

    private LinuxSystemPaths() {
    }
}

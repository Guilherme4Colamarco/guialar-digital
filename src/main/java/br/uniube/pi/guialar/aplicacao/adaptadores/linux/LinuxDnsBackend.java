package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Backend usado para aplicar DNS no Linux.
 */
public enum LinuxDnsBackend {
    NETWORK_MANAGER("NetworkManager"),
    NETPLAN("Netplan"),
    SYSTEMD_RESOLVED("systemd-resolved"),
    RESOLV_CONF("resolv.conf"),
    NIXOS_SNIPPET("NixOS (configuration.nix)");

    private final String rotulo;

    LinuxDnsBackend(String rotulo) {
        this.rotulo = rotulo;
    }

    public String getRotulo() {
        return rotulo;
    }
}

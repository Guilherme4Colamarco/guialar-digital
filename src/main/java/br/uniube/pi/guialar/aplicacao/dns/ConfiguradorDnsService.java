package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;

/**
 * Fachada Linux legada: delega para {@link ConfiguradorDnsLinuxService}.
 */
public class ConfiguradorDnsService {

    private final ConfiguradorDnsLinuxService linux;

    public ConfiguradorDnsService() {
        this.linux = new ConfiguradorDnsLinuxService();
    }

    public ConfiguradorDnsService(ConfiguradorDnsLinuxService linux) {
        this.linux = linux;
    }

    public ConfiguracaoDns configurar(InfoDistro distro) {
        return linux.configurar(distro);
    }

    public ConfiguracaoDns desfazer() {
        return linux.desfazer();
    }

    public ConfiguradorDnsLinuxService getLinux() {
        return linux;
    }
}

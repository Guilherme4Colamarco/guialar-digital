package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.distro.TipoDistro;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Escolhe o backend de DNS: NetworkManager ativo tem prioridade sobre Netplan em Ubuntu desktop.
 */
public class LinuxDnsBackendSelector {

    private final LinuxCommandRunner runner;
    private final LinuxOsRelease osRelease;
    private final Path netplanDir;
    private final Path osReleasePath;

    public LinuxDnsBackendSelector(LinuxCommandRunner runner) {
        this(runner, Path.of("/etc/os-release"), Path.of("/etc/netplan"));
    }

    public LinuxDnsBackendSelector(LinuxCommandRunner runner, Path osReleasePath, Path netplanDir) {
        this.runner = runner;
        this.osReleasePath = osReleasePath;
        this.netplanDir = netplanDir;
        LinuxOsRelease lido;
        try {
            lido = LinuxOsRelease.ler(osReleasePath);
        } catch (Exception e) {
            lido = new LinuxOsRelease(java.util.Map.of());
        }
        this.osRelease = lido;
    }

    public LinuxOsRelease getOsRelease() {
        return osRelease;
    }

    public boolean isNixOs() {
        return osRelease.isNixOs();
    }

    public boolean networkManagerAtivo() {
        if (!runner.comandoDisponivel("nmcli")) {
            return false;
        }
        try {
            if (runner.comandoDisponivel("systemctl")) {
                LinuxCommandResult r = runner.executar(false, "systemctl", "is-active", "NetworkManager");
                if (r.sucesso()) {
                    return true;
                }
            }
            LinuxCommandResult status = runner.executar(false, "nmcli", "-t", "-f", "RUNNING", "general");
            return status.stdout() != null && status.stdout().trim().equalsIgnoreCase("running");
        } catch (Exception e) {
            return false;
        }
    }

    public boolean existeNetplan() {
        return Files.isDirectory(netplanDir);
    }

    /**
     * Seleciona backend para aplicar DNS (exceto ramo NixOS snippet, decidido em {@link NixOsDnsSupport}).
     */
    public LinuxDnsBackend selecionar(InfoDistro distro) {
        if (networkManagerAtivo()) {
            return LinuxDnsBackend.NETWORK_MANAGER;
        }
        if (distro.getTipo() == TipoDistro.DEBIAN && existeNetplan()) {
            return LinuxDnsBackend.NETPLAN;
        }
        if ("systemd-resolved".equals(distro.getGerenciadorRede())
                || servicoResolvedAtivo()) {
            return LinuxDnsBackend.SYSTEMD_RESOLVED;
        }
        if (distro.getTipo() == TipoDistro.FEDORA) {
            return LinuxDnsBackend.NETWORK_MANAGER;
        }
        return LinuxDnsBackend.RESOLV_CONF;
    }

    private boolean servicoResolvedAtivo() {
        try {
            if (!runner.comandoDisponivel("systemctl")) {
                return false;
            }
            LinuxCommandResult r = runner.executar(false, "systemctl", "is-active", "systemd-resolved");
            return r.sucesso();
        } catch (Exception e) {
            return false;
        }
    }
}

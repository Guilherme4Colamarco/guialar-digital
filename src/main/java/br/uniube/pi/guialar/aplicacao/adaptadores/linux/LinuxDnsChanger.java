package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsLinuxService;
import br.uniube.pi.guialar.dominio.adaptadores.DnsChanger;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

/**
 * Implementação Linux do DnsChanger (delega para {@link ConfiguradorDnsLinuxService}).
 */
public class LinuxDnsChanger implements DnsChanger {

    private final InfoDistro distro;
    private final ConfiguradorDnsLinuxService linux;

    public LinuxDnsChanger(InfoDistro distro) {
        this(distro, new ConfiguradorDnsLinuxService());
    }

    public LinuxDnsChanger(InfoDistro distro, ConfiguradorDnsLinuxService linux) {
        this.distro = distro;
        this.linux = linux;
    }

    @Override
    public ConfiguracaoDns configurar(ServidorDns servidor) {
        return linux.configurar(distro, servidor);
    }

    @Override
    public boolean verificar() {
        try {
            ProcessBuilder pb = new ProcessBuilder("resolvectl", "status");
            pb.redirectErrorStream(true);
            Process process = pb.start();

            java.io.BufferedReader reader = new java.io.BufferedReader(
                new java.io.InputStreamReader(process.getInputStream())
            );

            String linha;
            boolean encontrou = false;

            while ((linha = reader.readLine()) != null) {
                if (LinuxDnsFamiliesDetector.textoContemAlvoGuialar(linha)) {
                    encontrou = true;
                    break;
                }
            }

            process.waitFor();
            return encontrou;

        } catch (Exception e) {
            try {
                java.nio.file.Path resolvConf = java.nio.file.Path.of("/etc/resolv.conf");
                if (java.nio.file.Files.exists(resolvConf)) {
                    String conteudo = java.nio.file.Files.readString(resolvConf);
                    return LinuxDnsFamiliesDetector.textoContemAlvoGuialar(conteudo);
                }
            } catch (Exception e2) {
                // Ignora
            }
            return false;
        }
    }

    @Override
    public ConfiguracaoDns reverter() {
        return linux.desfazer();
    }

    @Override
    public String getInstrucoesReversao() {
        return linux.getInstrucoesReversao(distro);
    }
}

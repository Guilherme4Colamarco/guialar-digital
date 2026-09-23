package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsChanger;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

/**
 * Serviço de configuração de DNS no Windows.
 * Delega para {@link WindowsDnsChanger} (PowerShell Set-DnsClientServerAddress).
 *
 * Elevação UAC só é solicitada na escrita; leitura/diagnóstico não pedem admin.
 */
public class ConfiguradorDnsWindowsService {

    private final WindowsDnsChanger changer;

    public ConfiguradorDnsWindowsService() {
        this(new WindowsDnsChanger());
    }

    public ConfiguradorDnsWindowsService(WindowsDnsChanger changer) {
        this.changer = changer;
    }

    public ConfiguracaoDns configurar() {
        return configurar(ServidorDns.getPadrao());
    }

    public ConfiguracaoDns configurar(ServidorDns servidor) {
        System.out.println("Configurando DNS no Windows via " + WindowsDnsChanger.METODO + "...");
        System.out.println("Alvo: " + servidor.getNome()
            + " (" + servidor.getPrimario() + " / " + servidor.getSecundario() + ")");
        System.out.println();

        ConfiguracaoDns resultado = changer.configurar(servidor);

        if (resultado.isAplicado()) {
            System.out.println("✓ " + resultado.getMensagem());
            System.out.println("  Manifesto salvo para Desfazer em %LOCALAPPDATA%\\GuiaLar\\");
        } else {
            System.out.println("○ " + resultado.getMensagem());
            System.out.println();
            System.out.println("O que ainda funciona sem DNS aplicado:");
            System.out.println("  • Diagnóstico do ambiente (sem admin)");
            System.out.println("  • Detecção de navegadores e guias (uBlock / Shields / DoH)");
            System.out.println();
            System.out.println("Instruções manuais (se tiver admin):");
            System.out.println(changer.getInstrucoesReversao().replace("desfazer", "aplicar/desfazer"));
            System.out.println("  Set-DnsClientServerAddress -InterfaceAlias \"Wi-Fi\" -ServerAddresses (`\""
                + servidor.getPrimario() + "`\",`\"" + servidor.getSecundario() + "`\")");
        }
        return resultado;
    }

    public ConfiguracaoDns desfazer() {
        return changer.reverter();
    }

    public WindowsDnsChanger getChanger() {
        return changer;
    }
}

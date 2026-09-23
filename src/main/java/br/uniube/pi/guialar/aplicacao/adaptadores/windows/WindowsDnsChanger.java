package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.aplicacao.plataforma.ProcessoUtil;
import br.uniube.pi.guialar.dominio.adaptadores.DnsChanger;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

/**
 * Troca de DNS no Windows via PowerShell ({@code Set-DnsClientServerAddress}).
 *
 * Estratégia:
 * - Descobre o adaptador de rede ativo com {@code Get-NetAdapter}.
 * - Faz backup do DNS atual antes de alterar.
 * - Define IPv4 + IPv6 do Cloudflare Families (1.1.1.3 / 1.0.0.3).
 * - Reversão com {@code -ResetServerAddresses}.
 *
 * Requer privilégios de administrador para efetivar a mudança.
 */
public class WindowsDnsChanger implements DnsChanger {

    @Override
    public ConfiguracaoDns configurar(ServidorDns servidor) {
        String script = String.join("; ",
            "$ErrorActionPreference='Stop'",
            "$ad = Get-NetAdapter | Where-Object {$_.Status -eq 'Up'} | Select-Object -First 1",
            "if(-not $ad){ Write-Error 'Nenhum adaptador de rede ativo'; exit 2 }",
            "$bkp = (Get-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -AddressFamily IPv4).ServerAddresses -join ','",
            "Write-Output ('BACKUP=' + $bkp)",
            "Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ServerAddresses @('"
                + servidor.getPrimario() + "','" + servidor.getSecundario() + "','"
                + servidor.getPrimarioIpv6() + "','" + servidor.getSecundarioIpv6() + "')",
            "Write-Output ('OK=' + $ad.Name)");

        System.out.println("[Windows] Configurando DNS via PowerShell (Set-DnsClientServerAddress)...");
        ProcessoUtil.Resultado r = ProcessoUtil.powershell(script);
        if (!r.getSaida().isEmpty()) {
            System.out.println(r.getSaida());
        }

        if (r.ok() && r.getSaida().contains("OK=")) {
            String adaptador = extrair(r.getSaida(), "OK=");
            System.out.println("  Adaptador configurado: " + adaptador);
            System.out.println("  Para desfazer depois: " + getInstrucoesReversao());
            return ConfiguracaoDns.sucesso(servidor, "Windows/Set-DnsClientServerAddress");
        }

        String erro = r.getSaida().isEmpty()
            ? "Falha ao configurar o DNS (é necessário executar como administrador)."
            : r.getSaida();
        return ConfiguracaoDns.erro("Windows/PowerShell", erro);
    }

    @Override
    public boolean verificar() {
        ProcessoUtil.Resultado r = ProcessoUtil.powershell(
            "(Get-DnsClientServerAddress -AddressFamily IPv4).ServerAddresses");
        return r.ok() && r.getSaida().contains(ServidorDns.getPadrao().getPrimario());
    }

    @Override
    public ConfiguracaoDns reverter() {
        String script = String.join("; ",
            "$ad = Get-NetAdapter | Where-Object {$_.Status -eq 'Up'} | Select-Object -First 1",
            "if($ad){ Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ResetServerAddresses }");
        ProcessoUtil.Resultado r = ProcessoUtil.powershell(script);
        return r.ok()
            ? ConfiguracaoDns.sucesso(ServidorDns.getPadrao(), "Windows/Reset")
            : ConfiguracaoDns.erro("Windows/PowerShell", r.getSaida());
    }

    @Override
    public String getInstrucoesReversao() {
        return "Get-NetAdapter | ? {$_.Status -eq 'Up'} | "
            + "% { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses }";
    }

    private String extrair(String saida, String prefixo) {
        for (String linha : saida.split("\\R")) {
            String l = linha.trim();
            if (l.startsWith(prefixo)) {
                return l.substring(prefixo.length()).trim();
            }
        }
        return "";
    }
}

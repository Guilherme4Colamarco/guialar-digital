package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.aplicacao.plataforma.ProcessoUtil;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.util.ArrayList;
import java.util.List;

/**
 * Smoke test de DNS no Windows.
 *
 * Equivale ao verificador do Linux, porém resolve nomes com PowerShell
 * ({@code Resolve-DnsName -Server 1.1.1.3}), com fallback para {@code nslookup}
 * e para a resolução nativa do Java.
 *
 * O resumo pode ser exibido reutilizando
 * {@code VerificacaoDnsService.exibirResumo(List)}, que só depende da lista de
 * resultados (independente do método de resolução).
 */
public class WindowsVerificacaoDnsService {

    private static final String IP_BLOQUEADO = "0.0.0.0";
    private static final String IP_BLOQUEADO_IPV6 = "::";

    public List<ResultadoVerificacao> verificar() {
        List<ResultadoVerificacao> resultados = new ArrayList<>();
        ServidorDns servidor = ServidorDns.getPadrao();

        System.out.println("[Windows] Conferindo DNS Cloudflare Families (Resolve-DnsName)...");

        resultados.add(testar(ServidorDns.URL_TESTE_MALWARE, servidor.getPrimario(), "IPv4", true));
        resultados.add(testar(ServidorDns.URL_TESTE_MALWARE, servidor.getPrimarioIpv6(), "IPv6", true));
        resultados.add(testar(ServidorDns.URL_TESTE_NUDITY, servidor.getPrimario(), "IPv4", true));
        resultados.add(testar(ServidorDns.URL_TESTE_NUDITY, servidor.getPrimarioIpv6(), "IPv6", true));
        resultados.add(testar("example.com", servidor.getPrimario(), "IPv4", false));
        resultados.add(testar("example.com", servidor.getPrimarioIpv6(), "IPv6", false));

        return resultados;
    }

    private ResultadoVerificacao testar(String url, String dnsServer, String tipoIp, boolean deveSerBloqueado) {
        System.out.print("  Testando " + url + " (" + tipoIp + ")... ");
        try {
            String ip = resolver(url, dnsServer, tipoIp);
            if (ip == null) {
                System.out.println("não foi possível resolver");
                return ResultadoVerificacao.erro(url, tipoIp, "Nenhum método de resolução disponível");
            }

            boolean bloqueado = ip.equals(IP_BLOQUEADO) || ip.equals(IP_BLOQUEADO_IPV6);
            if (deveSerBloqueado) {
                if (bloqueado) {
                    System.out.println("bloqueado (" + ip + ")");
                    return ResultadoVerificacao.bloqueioConfirmado(url, ip, tipoIp);
                }
                System.out.println("NÃO bloqueado (" + ip + ")");
                return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);
            }

            if (!bloqueado) {
                System.out.println("permitido (" + ip + ")");
                return ResultadoVerificacao.permitido(url, ip, tipoIp);
            }
            System.out.println("bloqueado incorretamente (" + ip + ")");
            return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);
        } catch (Exception e) {
            System.out.println("erro: " + e.getMessage());
            return ResultadoVerificacao.erro(url, tipoIp, e.getMessage());
        }
    }

    private String resolver(String url, String dnsServer, String tipoIp) {
        String tipo = "IPv6".equals(tipoIp) ? "AAAA" : "A";

        String script = "$r = Resolve-DnsName -Name '" + url + "' -Server " + dnsServer
            + " -Type " + tipo + " -DnsOnly -ErrorAction SilentlyContinue; "
            + "if($r){ ($r | Where-Object {$_.IPAddress} | Select-Object -First 1).IPAddress }";
        ProcessoUtil.Resultado r = ProcessoUtil.powershell(script);
        if (r.ok() && !r.getSaida().isBlank()) {
            return primeiraLinha(r.getSaida());
        }

        ProcessoUtil.Resultado ns = ProcessoUtil.executar("nslookup", "-type=" + tipo, url, dnsServer);
        String ipNs = extrairIpNslookup(ns.getSaida());
        if (ipNs != null) {
            return ipNs;
        }

        try {
            return java.net.InetAddress.getByName(url).getHostAddress();
        } catch (Exception e) {
            return null;
        }
    }

    private String primeiraLinha(String saida) {
        for (String linha : saida.split("\\R")) {
            String l = linha.trim();
            if (!l.isEmpty()) {
                return l;
            }
        }
        return null;
    }

    private String extrairIpNslookup(String saida) {
        // Ignora as linhas do próprio servidor DNS; pega o primeiro "Address" após "Name".
        boolean depoisDoNome = false;
        for (String linha : saida.split("\\R")) {
            String l = linha.trim();
            if (l.startsWith("Name:") || l.startsWith("Nome:")) {
                depoisDoNome = true;
                continue;
            }
            if (depoisDoNome && (l.startsWith("Address:") || l.startsWith("Addresses:") || l.startsWith("Endereço:"))) {
                int idx = l.indexOf(':');
                if (idx >= 0 && idx + 1 < l.length()) {
                    return l.substring(idx + 1).trim();
                }
            }
        }
        return null;
    }
}

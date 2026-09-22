package br.uniube.pi.guialar.aplicacao.verificacao;

import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.ArrayList;
import java.util.List;

/**
 * Serviço para verificação pós-DNS (smoke test).
 * 
 * Testa se o DNS Cloudflare Families está funcionando corretamente:
 * - malware.testcategory.com deve retornar 0.0.0.0 (bloqueado)
 * - nudity.testcategory.com deve retornar 0.0.0.0 (bloqueado)
 * - example.com deve funcionar normalmente
 * 
 * Testa IPv4 e IPv6.
 * 
 * Funciona em Docker e VM.
 */
public class VerificacaoDnsService {

    private static final String IP_BLOQUEADO = "0.0.0.0";
    private static final String IP_BLOQUEADO_IPv6 = "::";

    /**
     * Executa verificação completa do DNS.
     * 
     * @return Lista de resultados da verificação
     */
    public List<ResultadoVerificacao> verificar() {
        List<ResultadoVerificacao> resultados = new ArrayList<>();

        ServidorDns servidor = ServidorDns.getPadrao();

        System.out.println("🔍 Verificando DNS Cloudflare Families...");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();

        // Testar malware (IPv4)
        resultados.add(testarBloqueio(
            ServidorDns.URL_TESTE_MALWARE,
            servidor.getPrimario(),
            "IPv4",
            true
        ));

        // Testar malware (IPv6)
        resultados.add(testarBloqueio(
            ServidorDns.URL_TESTE_MALWARE,
            servidor.getPrimarioIpv6(),
            "IPv6",
            true
        ));

        // Testar nudity (IPv4)
        resultados.add(testarBloqueio(
            ServidorDns.URL_TESTE_NUDITY,
            servidor.getPrimario(),
            "IPv4",
            true
        ));

        // Testar nudity (IPv6)
        resultados.add(testarBloqueio(
            ServidorDns.URL_TESTE_NUDITY,
            servidor.getPrimarioIpv6(),
            "IPv6",
            true
        ));

        // Testar site normal (IPv4)
        resultados.add(testarBloqueio(
            "example.com",
            servidor.getPrimario(),
            "IPv4",
            false
        ));

        // Testar site normal (IPv6)
        resultados.add(testarBloqueio(
            "example.com",
            servidor.getPrimarioIpv6(),
            "IPv6",
            false
        ));

        return resultados;
    }

    /**
     * Testa bloqueio de uma URL específica.
     * 
     * @param url URL a testar
     * @param dnsServer Servidor DNS a usar
     * @param tipoIp "IPv4" ou "IPv6"
     * @param deveSer blocked true se deve estar bloqueado, false se deve funcionar
     * @return Resultado da verificação
     */
    private ResultadoVerificacao testarBloqueio(String url, String dnsServer, 
                                                String tipoIp, boolean deveSerBloqueado) {
        System.out.print("  Testando " + url + " (" + tipoIp + ")... ");

        try {
            // Tentar com dig primeiro
            String ip = testarComDig(url, dnsServer, tipoIp);
            
            if (ip == null) {
                // Se dig não funcionar, tentar com resolvectl
                ip = testarComResolvectl(url, tipoIp);
            }

            if (ip == null) {
                // Se nada funcionar, tentar lookup Java nativo
                ip = testarComJava(url);
            }

            if (ip == null) {
                System.out.println("⚠️ Não foi possível resolver");
                return ResultadoVerificacao.erro(url, tipoIp, "Nenhum método de resolução disponível");
            }

            // Verificar se o resultado é esperado
            boolean bloqueado = ip.equals(IP_BLOQUEADO) || ip.equals(IP_BLOQUEADO_IPv6);

            if (deveSerBloqueado) {
                if (bloqueado) {
                    System.out.println("✅ Bloqueado (" + ip + ")");
                    return ResultadoVerificacao.bloqueioConfirmado(url, ip, tipoIp);
                } else {
                    System.out.println("❌ NÃO bloqueado (" + ip + ")");
                    return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);
                }
            } else {
                if (!bloqueado) {
                    System.out.println("✅ Permitido (" + ip + ")");
                    return ResultadoVerificacao.permitido(url, ip, tipoIp);
                } else {
                    System.out.println("❌ Bloqueado incorretamente (" + ip + ")");
                    return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);
                }
            }

        } catch (Exception e) {
            System.out.println("⚠️ Erro: " + e.getMessage());
            return ResultadoVerificacao.erro(url, tipoIp, e.getMessage());
        }
    }

    /**
     * Testa usando comando dig.
     */
    private String testarComDig(String url, String dnsServer, String tipoIp) {
        try {
            String recordType = tipoIp.equals("IPv6") ? "AAAA" : "A";
            
            ProcessBuilder pb = new ProcessBuilder(
                "dig", "@" + dnsServer, url, recordType, "+short"
            );
            pb.redirectErrorStream(true);
            Process process = pb.start();

            BufferedReader reader = new BufferedReader(
                new InputStreamReader(process.getInputStream())
            );

            String linha;
            String primeiraResposta = null;
            
            while ((linha = reader.readLine()) != null) {
                linha = linha.trim();
                if (!linha.isEmpty() && !linha.startsWith(";")) {
                    primeiraResposta = linha;
                    break;
                }
            }

            process.waitFor();

            return primeiraResposta;

        } catch (Exception e) {
            return null;
        }
    }

    /**
     * Testa usando comando resolvectl (systemd-resolved).
     */
    private String testarComResolvectl(String url, String tipoIp) {
        try {
            ProcessBuilder pb = new ProcessBuilder("resolvectl", "query", url);
            pb.redirectErrorStream(true);
            Process process = pb.start();

            BufferedReader reader = new BufferedReader(
                new InputStreamReader(process.getInputStream())
            );

            String linha;
            while ((linha = reader.readLine()) != null) {
                // Procurar linha com IP
                if (linha.contains(":") && (linha.contains("IN A") || linha.contains("IN AAAA"))) {
                    String[] partes = linha.trim().split("\\s+");
                    if (partes.length > 0) {
                        return partes[0];
                    }
                }
            }

            process.waitFor();
            return null;

        } catch (Exception e) {
            return null;
        }
    }

    /**
     * Testa usando lookup Java nativo (fallback).
     */
    private String testarComJava(String url) {
        try {
            java.net.InetAddress addr = java.net.InetAddress.getByName(url);
            return addr.getHostAddress();
        } catch (Exception e) {
            return null;
        }
    }

    /**
     * Exibe resumo dos resultados.
     */
    public void exibirResumo(List<ResultadoVerificacao> resultados) {
        System.out.println();
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println("RESUMO DA VERIFICAÇÃO:");
        System.out.println("─────────────────────────────────────────────────────────────────");

        int total = resultados.size();
        int sucesso = 0;
        int falhas = 0;

        for (ResultadoVerificacao r : resultados) {
            if (r.isSucesso()) {
                sucesso++;
            } else {
                falhas++;
                System.out.println("❌ " + r.getMensagem());
            }
        }

        System.out.println();
        System.out.println("Total de testes: " + total);
        System.out.println("Sucesso: " + sucesso);
        System.out.println("Falhas: " + falhas);
        System.out.println();

        if (falhas == 0) {
            System.out.println("✅ TODOS OS TESTES PASSARAM!");
            System.out.println("   O DNS Cloudflare Families está funcionando corretamente.");
        } else {
            System.out.println("⚠️ ALGUNS TESTES FALHARAM!");
            System.out.println("   Verifique a configuração do DNS.");
        }

        System.out.println();
    }
}

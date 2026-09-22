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
 * Testa via STUB LOCAL (resolvectl query ou dig @127.0.0.53) para provar
 * que o SISTEMA está usando o DNS Families, não testando diretamente contra 1.1.1.3.
 * 
 * Funciona em Docker e VM.
 */
public class VerificacaoDnsService {

    private static final String IP_BLOQUEADO = "0.0.0.0";
    private static final String IP_BLOQUEADO_IPv6 = "::";

    /**
     * Executa verificação completa do DNS.
     * Testa via STUB do sistema, não diretamente contra 1.1.1.3
     * 
     * @return Lista de resultados da verificação
     */
    public List<ResultadoVerificacao> verificar() {
        List<ResultadoVerificacao> resultados = new ArrayList<>();

        System.out.println("🔍 Verificando DNS Cloudflare Families VIA STUB DO SISTEMA...");
        System.out.println("   (não testando diretamente contra 1.1.1.3, mas via stub local)");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();

        // Testar malware (IPv4)
        resultados.add(testarBloqueioViaStub(
            ServidorDns.URL_TESTE_MALWARE,
            "IPv4",
            true
        ));

        // Testar malware (IPv6)
        resultados.add(testarBloqueioViaStub(
            ServidorDns.URL_TESTE_MALWARE,
            "IPv6",
            true
        ));

        // Testar nudity (IPv4)
        resultados.add(testarBloqueioViaStub(
            ServidorDns.URL_TESTE_NUDITY,
            "IPv4",
            true
        ));

        // Testar nudity (IPv6)
        resultados.add(testarBloqueioViaStub(
            ServidorDns.URL_TESTE_NUDITY,
            "IPv6",
            true
        ));

        // Testar site normal (IPv4)
        resultados.add(testarBloqueioViaStub(
            "example.com",
            "IPv4",
            false
        ));

        // Testar site normal (IPv6)
        resultados.add(testarBloqueioViaStub(
            "example.com",
            "IPv6",
            false
        ));

        return resultados;
    }
    
    /**
     * Testa bloqueio VIA STUB do sistema (resolvectl query ou dig @127.0.0.53).
     * Isso prova que o SISTEMA está usando o DNS Families.
     */
    private ResultadoVerificacao testarBloqueioViaStub(String url, String tipoIp, boolean deveSerBloqueado) {
        System.out.print("  Testando " + url + " (" + tipoIp + ") via stub... ");

        try {
            // 1. Tentar com resolvectl query (prova que o sistema usa o DNS configurado)
            String ip = testarComResolvectl(url, tipoIp);
            
            // 2. Se não funcionar, tentar dig @127.0.0.53 (stub do systemd-resolved)
            if (ip == null) {
                ip = testarComDigStub(url, tipoIp);
            }

            // 3. Se nada funcionar, tentar lookup Java nativo (usa o resolver do sistema)
            if (ip == null) {
                ip = testarComJava(url);
            }

            if (ip == null) {
                System.out.println("⚠️ Não foi possível resolver via stub");
                return ResultadoVerificacao.erro(url, tipoIp, "Nenhum método de resolução via stub disponível");
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
     * Testa usando dig via STUB (127.0.0.53 do systemd-resolved).
     * Isso prova que o sistema está usando o DNS configurado.
     */
    private String testarComDigStub(String url, String tipoIp) {
        try {
            String recordType = tipoIp.equals("IPv6") ? "AAAA" : "A";
            
            // Testar via stub do systemd-resolved
            ProcessBuilder pb = new ProcessBuilder(
                "dig", "@127.0.0.53", url, recordType, "+short"
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

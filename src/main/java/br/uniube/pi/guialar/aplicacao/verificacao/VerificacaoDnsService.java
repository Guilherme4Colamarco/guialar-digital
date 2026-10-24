package br.uniube.pi.guialar.aplicacao.verificacao;

import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.ArrayList;
import java.util.List;

/**
 * Smoke test pós-DNS via stub local (resolvectl / dig @127.0.0.53 / Java).
 *
 * Cloudflare Families: malware/nudity → 0.0.0.0
 * AdGuard: domínios de anúncio → NXDOMAIN ou 0.0.0.0
 */
public class VerificacaoDnsService {

    private static final String IP_BLOQUEADO = "0.0.0.0";
    private static final String IP_BLOQUEADO_IPV6 = "::";
    private static final String DOMINIO_NORMAL = "example.com";

    public static boolean protecaoConfirmada(List<ResultadoVerificacao> resultados) {
        return protecaoConfirmada(resultados, ServidorDns.getPadrao());
    }

    public static boolean protecaoConfirmada(List<ResultadoVerificacao> resultados, ServidorDns servidor) {
        if (resultados == null || resultados.isEmpty()) {
            return false;
        }
        ServidorDns alvo = servidor != null ? servidor : ServidorDns.getPadrao();
        String primeiroBloqueio = alvo.getDominiosTesteBloqueio()[0];
        boolean bloqueioOk = false;
        boolean exampleOk = false;
        for (ResultadoVerificacao r : resultados) {
            if (!r.isSucesso()) {
                continue;
            }
            if (primeiroBloqueio.equals(r.getUrl()) && "IPv4".equals(r.getMetodo()) && r.isBloqueado()) {
                bloqueioOk = true;
            }
            if (DOMINIO_NORMAL.equals(r.getUrl()) && "IPv4".equals(r.getMetodo()) && !r.isBloqueado()) {
                exampleOk = true;
            }
        }
        return bloqueioOk && exampleOk;
    }

    public List<ResultadoVerificacao> verificar() {
        return verificar(ServidorDns.getPadrao());
    }

    public List<ResultadoVerificacao> verificar(ServidorDns servidor) {
        ServidorDns alvo = servidor != null ? servidor : ServidorDns.getPadrao();
        List<ResultadoVerificacao> resultados = new ArrayList<>();

        System.out.println("🔍 Verificando DNS " + alvo.getNome() + " VIA STUB DO SISTEMA...");
        System.out.println("   (não testando diretamente contra " + alvo.getPrimario() + ")");
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println();

        for (String dominio : alvo.getDominiosTesteBloqueio()) {
            resultados.add(testarBloqueioViaStub(dominio, "IPv4", true, alvo.isNxdomainComoBloqueio()));
            resultados.add(testarBloqueioViaStub(dominio, "IPv6", true, alvo.isNxdomainComoBloqueio()));
        }

        resultados.add(testarBloqueioViaStub(DOMINIO_NORMAL, "IPv4", false, false));
        resultados.add(testarBloqueioViaStub(DOMINIO_NORMAL, "IPv6", false, false));

        return resultados;
    }

    private ResultadoVerificacao testarBloqueioViaStub(
            String url, String tipoIp, boolean deveSerBloqueado, boolean nxdomainComoBloqueio) {
        System.out.print("  Testando " + url + " (" + tipoIp + ") via stub... ");

        try {
            String ip = testarComResolvectl(url, tipoIp);
            if (ip == null) {
                ip = testarComDigStub(url, tipoIp);
            }
            if (ip == null) {
                ip = testarComJava(url);
            }

            if (ip == null) {
                if (deveSerBloqueado && nxdomainComoBloqueio) {
                    System.out.println("✅ Bloqueado (NXDOMAIN/sem resposta)");
                    return ResultadoVerificacao.bloqueioConfirmado(url, "NXDOMAIN", tipoIp);
                }
                System.out.println("⚠️ Não foi possível resolver via stub");
                return ResultadoVerificacao.erro(url, tipoIp, "Nenhum método de resolução via stub disponível");
            }

            boolean bloqueado = ip.equals(IP_BLOQUEADO)
                || ip.equals(IP_BLOQUEADO_IPV6)
                || "NXDOMAIN".equalsIgnoreCase(ip);

            if (deveSerBloqueado) {
                if (bloqueado) {
                    System.out.println("✅ Bloqueado (" + ip + ")");
                    return ResultadoVerificacao.bloqueioConfirmado(url, ip, tipoIp);
                }
                System.out.println("❌ NÃO bloqueado (" + ip + ")");
                return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);
            }
            if (!bloqueado) {
                System.out.println("✅ Permitido (" + ip + ")");
                return ResultadoVerificacao.permitido(url, ip, tipoIp);
            }
            System.out.println("❌ Bloqueado incorretamente (" + ip + ")");
            return ResultadoVerificacao.naoFuncionou(url, ip, tipoIp);

        } catch (Exception e) {
            System.out.println("⚠️ Erro: " + e.getMessage());
            return ResultadoVerificacao.erro(url, tipoIp, e.getMessage());
        }
    }

    private String testarComDigStub(String url, String tipoIp) {
        try {
            String recordType = tipoIp.equals("IPv6") ? "AAAA" : "A";
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
                String lower = linha.toLowerCase();
                if (lower.contains("nxdomain") || lower.contains("name error")
                        || lower.contains("does not exist")) {
                    process.waitFor();
                    return "NXDOMAIN";
                }
                if (linha.contains(":") && (linha.contains("IN A") || linha.contains("IN AAAA"))) {
                    String[] partes = linha.trim().split("\\s+");
                    if (partes.length > 0) {
                        process.waitFor();
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

    private String testarComJava(String url) {
        try {
            java.net.InetAddress addr = java.net.InetAddress.getByName(url);
            return addr.getHostAddress();
        } catch (java.net.UnknownHostException e) {
            return null;
        } catch (Exception e) {
            return null;
        }
    }

    public void exibirResumo(List<ResultadoVerificacao> resultados) {
        exibirResumo(resultados, ServidorDns.getPadrao());
    }

    public void exibirResumo(List<ResultadoVerificacao> resultados, ServidorDns servidor) {
        ServidorDns alvo = servidor != null ? servidor : ServidorDns.getPadrao();
        System.out.println();
        System.out.println("─────────────────────────────────────────────────────────────────");
        System.out.println("RESUMO DA VERIFICAÇÃO (" + alvo.getNome() + "):");
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
            System.out.println("   O DNS " + alvo.getNome() + " parece ativo no stub do sistema.");
        } else {
            System.out.println("⚠️ ALGUNS TESTES FALHARAM!");
            System.out.println("   Verifique a configuração do DNS.");
        }
        System.out.println();
    }
}

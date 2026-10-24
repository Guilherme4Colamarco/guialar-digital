package br.uniube.pi.guialar.aplicacao.dns;

import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

import java.awt.Desktop;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.TimeUnit;

/**
 * Integração com uBlockDNS (ublockdns.com) — DNS para todos os apps no PC.
 *
 * <p>Não afiliado ao uBlock Origin. O cliente local escuta em 127.0.0.1:53 e
 * encaminha DoH com o perfil da dashboard. GuiaLar não embute o binário: abre o
 * site e, com ID de perfil + autorização, dispara o instalador oficial.
 */
public class UblockDnsAssistenteService {

    public static final String SITE = ServidorDns.UBLOCKDNS_SITE;

    public boolean abrirDashboard() {
        try {
            if (Desktop.isDesktopSupported() && Desktop.getDesktop().isSupported(Desktop.Action.BROWSE)) {
                Desktop.getDesktop().browse(URI.create(SITE));
                return true;
            }
        } catch (Exception ignored) {
        }
        return false;
    }

    public boolean clienteInstalado() {
        return !caminhoBinario().isEmpty() || comandoExiste("ublockdns");
    }

    public String statusResumo() {
        if (!clienteInstalado()) {
            return "Cliente uBlockDNS não encontrado. Crie um perfil em " + SITE
                + " e escolha Linux ou Windows → All apps on this computer.";
        }
        String out = executarCaptura(List.of("ublockdns", "status"), 8);
        if (out == null || out.isBlank()) {
            return "Cliente presente, mas 'ublockdns status' não respondeu.";
        }
        return out.strip();
    }

    public boolean pareceAtivo() {
        String s = statusResumo().toLowerCase(Locale.ROOT);
        if (s.contains("não encontrado") || s.contains("nao encontrado")) {
            return false;
        }
        return s.contains("ready") || s.contains("active") || s.contains("running")
            || s.contains("ok") || s.contains("listening");
    }

    /**
     * Instala/ativa o cliente oficial com o profile-id (requer rede + admin).
     */
    public ConfiguracaoDns instalar(TipoSistema sistema, String profileId) {
        String id = normalizarProfileId(profileId);
        if (id == null) {
            return ConfiguracaoDns.erro("uBlockDNS",
                "Informe o Profile ID da dashboard (ublockdns.com → Setup).");
        }
        if (sistema != null && sistema.isWindows()) {
            return instalarWindows(id);
        }
        return instalarUnix(id);
    }

    public String comandoInstalacaoSugerido(TipoSistema sistema, String profileId) {
        String id = normalizarProfileId(profileId);
        String placeholder = id != null ? id : "<profile-id>";
        if (sistema != null && sistema.isWindows()) {
            return "irm https://ublockdns.com/install?id=" + placeholder + " | iex";
        }
        return "curl -sSfL https://ublockdns.com/install.sh | sh -s -- " + placeholder;
    }

    public String textoAjudaGui() {
        return """
            uBlockDNS (todos os apps neste computador)
            
            1. Abra https://ublockdns.com e crie/entre na conta
            2. Em Setup, escolha Linux ou Windows → “All apps on this computer”
            3. Copie o Profile ID e cole no GuiaLar
            4. Autorize a instalação (pede senha/admin)
            
            Também há opções só no Chrome/Firefox, roteador, iOS e Android no site.
            Não afiliado ao uBlock Origin (extensão).
            """.strip();
    }

    private ConfiguracaoDns instalarUnix(String profileId) {
        List<String> cmd = List.of(
            "sh", "-c",
            "curl -sSfL https://ublockdns.com/install.sh | sh -s -- " + shellQuote(profileId)
        );
        int code = executar(cmd, 180);
        if (code != 0) {
            return ConfiguracaoDns.naoAplicado("uBlockDNS",
                "Instalador retornou código " + code + ". Rode manualmente:\n"
                    + comandoInstalacaoSugerido(null, profileId));
        }
        if (pareceAtivo() || clienteInstalado()) {
            return ConfiguracaoDns.sucesso(ServidorDns.UBLOCKDNS, "uBlockDNS install.sh");
        }
        return ConfiguracaoDns.sucessoComAviso(ServidorDns.UBLOCKDNS, "uBlockDNS install.sh",
            "Instalador terminou; confirme com 'ublockdns status' ou Verificar de novo.");
    }

    private ConfiguracaoDns instalarWindows(String profileId) {
        List<String> cmd = List.of(
            "powershell",
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-Command",
            "irm https://ublockdns.com/install?id=" + profileId + " | iex"
        );
        int code = executar(cmd, 180);
        if (code != 0) {
            return ConfiguracaoDns.naoAplicado("uBlockDNS",
                "Instalador Windows retornou " + code + ". Como Admin:\n"
                    + comandoInstalacaoSugerido(
                        br.uniube.pi.guialar.dominio.sistema.TipoSistema.WINDOWS, profileId));
        }
        return ConfiguracaoDns.sucesso(ServidorDns.UBLOCKDNS, "uBlockDNS install (Windows)");
    }

    private static String normalizarProfileId(String raw) {
        if (raw == null) {
            return null;
        }
        String id = raw.trim();
        if (id.isEmpty() || id.contains(" ") || id.contains("<")) {
            return null;
        }
        if (!id.matches("[A-Za-z0-9._-]{4,128}")) {
            return null;
        }
        return id;
    }

    private static String shellQuote(String s) {
        return "'" + s.replace("'", "'\\''") + "'";
    }

    private String caminhoBinario() {
        String[] candidatos = {
            "/usr/local/bin/ublockdns",
            "/usr/bin/ublockdns",
            System.getenv("ProgramFiles") != null
                ? System.getenv("ProgramFiles") + "\\uBlockDNS\\ublockdns.exe" : ""
        };
        for (String c : candidatos) {
            if (c != null && !c.isEmpty() && java.nio.file.Files.isExecutable(java.nio.file.Path.of(c))) {
                return c;
            }
        }
        return "";
    }

    private boolean comandoExiste(String nome) {
        try {
            Process p = new ProcessBuilder("sh", "-c", "command -v " + nome)
                .redirectErrorStream(true).start();
            boolean ok = p.waitFor(3, TimeUnit.SECONDS) && p.exitValue() == 0;
            p.destroyForcibly();
            return ok;
        } catch (Exception e) {
            return false;
        }
    }

    private int executar(List<String> cmd, int timeoutSeg) {
        try {
            ProcessBuilder pb = new ProcessBuilder(cmd);
            pb.redirectErrorStream(true);
            Process p = pb.start();
            try (BufferedReader r = new BufferedReader(
                    new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    System.out.println(line);
                }
            }
            if (!p.waitFor(timeoutSeg, TimeUnit.SECONDS)) {
                p.destroyForcibly();
                return -1;
            }
            return p.exitValue();
        } catch (Exception e) {
            System.err.println("Falha ao executar instalador uBlockDNS: " + e.getMessage());
            return -1;
        }
    }

    private String executarCaptura(List<String> cmd, int timeoutSeg) {
        try {
            ProcessBuilder pb = new ProcessBuilder(cmd);
            pb.redirectErrorStream(true);
            Process p = pb.start();
            List<String> lines = new ArrayList<>();
            try (BufferedReader r = new BufferedReader(
                    new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8))) {
                String line;
                while ((line = r.readLine()) != null) {
                    lines.add(line);
                }
            }
            p.waitFor(timeoutSeg, TimeUnit.SECONDS);
            return String.join("\n", lines);
        } catch (Exception e) {
            return null;
        }
    }
}

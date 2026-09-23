package br.uniube.pi.guialar.dominio.diagnostico;

import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * Diagnóstico somente leitura do ambiente (sem privilégios administrativos).
 * Usado no smoke test em PCs de laboratório com conta limitada.
 */
public class DiagnosticoAmbiente {

    private final TipoSistema tipoSistema;
    private final String nomeSistema;
    private final String versaoSistema;
    private final String arquitetura;
    private final boolean processoElevado;
    private final boolean podeElevar;
    private final StatusDns statusDns;
    private final List<String> servidoresDnsAtuais;
    private final String notaDns;
    private final List<Navegador> navegadores;
    private final List<String> notasConectividade;
    private final List<String> oQueFunciona;
    private final String caminhoDadosUsuario;

    private DiagnosticoAmbiente(Builder b) {
        this.tipoSistema = b.tipoSistema;
        this.nomeSistema = b.nomeSistema;
        this.versaoSistema = b.versaoSistema;
        this.arquitetura = b.arquitetura;
        this.processoElevado = b.processoElevado;
        this.podeElevar = b.podeElevar;
        this.statusDns = b.statusDns;
        this.servidoresDnsAtuais = Collections.unmodifiableList(new ArrayList<>(b.servidoresDnsAtuais));
        this.notaDns = b.notaDns;
        this.navegadores = Collections.unmodifiableList(new ArrayList<>(b.navegadores));
        this.notasConectividade = Collections.unmodifiableList(new ArrayList<>(b.notasConectividade));
        this.oQueFunciona = Collections.unmodifiableList(new ArrayList<>(b.oQueFunciona));
        this.caminhoDadosUsuario = b.caminhoDadosUsuario;
    }

    public TipoSistema getTipoSistema() {
        return tipoSistema;
    }

    public String getNomeSistema() {
        return nomeSistema;
    }

    public String getVersaoSistema() {
        return versaoSistema;
    }

    public String getArquitetura() {
        return arquitetura;
    }

    public boolean isProcessoElevado() {
        return processoElevado;
    }

    public boolean isPodeElevar() {
        return podeElevar;
    }

    public StatusDns getStatusDns() {
        return statusDns;
    }

    public List<String> getServidoresDnsAtuais() {
        return servidoresDnsAtuais;
    }

    public String getNotaDns() {
        return notaDns;
    }

    public List<Navegador> getNavegadores() {
        return navegadores;
    }

    public List<String> getNotasConectividade() {
        return notasConectividade;
    }

    public List<String> getOQueFunciona() {
        return oQueFunciona;
    }

    public String getCaminhoDadosUsuario() {
        return caminhoDadosUsuario;
    }

    /**
     * Texto completo em português para CLI/GUI.
     */
    public String formatarRelatorio() {
        StringBuilder sb = new StringBuilder();
        sb.append("=== Diagnóstico do ambiente (somente leitura) ===\n");
        sb.append("Sistema: ").append(nomeSistema).append(" ").append(versaoSistema).append('\n');
        sb.append("Arquitetura: ").append(arquitetura).append('\n');
        sb.append("Processo elevado (admin): ").append(processoElevado ? "sim" : "não").append('\n');
        sb.append("Pode solicitar elevação (UAC): ").append(podeElevar ? "sim (quando aplicar DNS)" : "não / bloqueado").append('\n');
        sb.append("Status DNS GuiaLar: ").append(statusDns.getRotuloPt()).append('\n');
        if (notaDns != null && !notaDns.isBlank()) {
            sb.append("DNS atual: ").append(notaDns).append('\n');
        }
        if (!servidoresDnsAtuais.isEmpty()) {
            sb.append("Servidores DNS detectados:\n");
            for (String dns : servidoresDnsAtuais) {
                sb.append("  • ").append(dns).append('\n');
            }
        }
        sb.append("Navegadores detectados: ").append(navegadores.size()).append('\n');
        for (Navegador nav : navegadores) {
            sb.append("  • ").append(nav.getNome())
              .append(" (").append(nav.getTipo().getNomeExibicao()).append(")\n");
        }
        if (!notasConectividade.isEmpty()) {
            sb.append("Conectividade:\n");
            for (String n : notasConectividade) {
                sb.append("  • ").append(n).append('\n');
            }
        }
        sb.append("O que ainda funciona sem admin:\n");
        for (String item : oQueFunciona) {
            sb.append("  • ").append(item).append('\n');
        }
        if (caminhoDadosUsuario != null) {
            sb.append("Dados do GuiaLar: ").append(caminhoDadosUsuario).append('\n');
        }
        return sb.toString();
    }

    public static Builder builder() {
        return new Builder();
    }

    public static final class Builder {
        private TipoSistema tipoSistema = TipoSistema.DESCONHECIDO;
        private String nomeSistema = "Desconhecido";
        private String versaoSistema = "N/A";
        private String arquitetura = System.getProperty("os.arch", "N/A");
        private boolean processoElevado;
        private boolean podeElevar;
        private StatusDns statusDns = StatusDns.DESCONHECIDO;
        private final List<String> servidoresDnsAtuais = new ArrayList<>();
        private String notaDns = "";
        private final List<Navegador> navegadores = new ArrayList<>();
        private final List<String> notasConectividade = new ArrayList<>();
        private final List<String> oQueFunciona = new ArrayList<>();
        private String caminhoDadosUsuario;

        public Builder tipoSistema(TipoSistema v) { this.tipoSistema = v; return this; }
        public Builder nomeSistema(String v) { this.nomeSistema = v; return this; }
        public Builder versaoSistema(String v) { this.versaoSistema = v; return this; }
        public Builder arquitetura(String v) { this.arquitetura = v; return this; }
        public Builder processoElevado(boolean v) { this.processoElevado = v; return this; }
        public Builder podeElevar(boolean v) { this.podeElevar = v; return this; }
        public Builder statusDns(StatusDns v) { this.statusDns = v; return this; }
        public Builder servidoresDnsAtuais(List<String> v) {
            this.servidoresDnsAtuais.clear();
            if (v != null) {
                this.servidoresDnsAtuais.addAll(v);
            }
            return this;
        }
        public Builder notaDns(String v) { this.notaDns = v; return this; }
        public Builder navegadores(List<Navegador> v) {
            this.navegadores.clear();
            if (v != null) {
                this.navegadores.addAll(v);
            }
            return this;
        }
        public Builder notasConectividade(List<String> v) {
            this.notasConectividade.clear();
            if (v != null) {
                this.notasConectividade.addAll(v);
            }
            return this;
        }
        public Builder oQueFunciona(List<String> v) {
            this.oQueFunciona.clear();
            if (v != null) {
                this.oQueFunciona.addAll(v);
            }
            return this;
        }
        public Builder caminhoDadosUsuario(String v) { this.caminhoDadosUsuario = v; return this; }

        public DiagnosticoAmbiente build() {
            return new DiagnosticoAmbiente(this);
        }
    }
}

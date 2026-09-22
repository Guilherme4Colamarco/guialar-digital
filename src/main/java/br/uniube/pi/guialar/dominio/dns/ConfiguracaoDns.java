package br.uniube.pi.guialar.dominio.dns;

/**
 * Representa a configuração de DNS aplicada ao sistema.
 */
public class ConfiguracaoDns {
    private ServidorDns servidor;
    private boolean aplicado;
    private String metodoConfiguracao;
    private String mensagem;

    public ConfiguracaoDns(ServidorDns servidor, boolean aplicado, String metodoConfiguracao, String mensagem) {
        this.servidor = servidor;
        this.aplicado = aplicado;
        this.metodoConfiguracao = metodoConfiguracao;
        this.mensagem = mensagem;
    }

    public ServidorDns getServidor() {
        return servidor;
    }

    public boolean isAplicado() {
        return aplicado;
    }

    public String getMetodoConfiguracao() {
        return metodoConfiguracao;
    }

    public String getMensagem() {
        return mensagem;
    }

    public static ConfiguracaoDns sucesso(ServidorDns servidor, String metodo) {
        return new ConfiguracaoDns(
            servidor,
            true,
            metodo,
            String.format("DNS configurado com sucesso: %s (%s / %s)", 
                servidor.getNome(), 
                servidor.getPrimario(), 
                servidor.getSecundario())
        );
    }

    public static ConfiguracaoDns erro(String metodo, String mensagemErro) {
        return new ConfiguracaoDns(
            null,
            false,
            metodo,
            "Erro ao configurar DNS: " + mensagemErro
        );
    }

    /**
     * DNS não foi aplicado (sem privilégios, GPO, UAC negado).
     * O sistema permanece inalterado — mensagem clara em português.
     */
    public static ConfiguracaoDns naoAplicado(String metodo, String motivo) {
        return new ConfiguracaoDns(
            null,
            false,
            metodo,
            "DNS não aplicado: " + motivo
        );
    }

    /**
     * Sucesso com aviso adicional (ex.: manifesto não salvo).
     */
    public static ConfiguracaoDns sucessoComAviso(ServidorDns servidor, String metodo, String aviso) {
        return new ConfiguracaoDns(
            servidor,
            true,
            metodo,
            String.format("DNS configurado: %s (%s / %s). Aviso: %s",
                servidor.getNome(),
                servidor.getPrimario(),
                servidor.getSecundario(),
                aviso)
        );
    }
}

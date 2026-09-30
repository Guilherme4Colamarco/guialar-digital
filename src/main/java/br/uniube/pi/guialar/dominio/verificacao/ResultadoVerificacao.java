package br.uniube.pi.guialar.dominio.verificacao;

/**
 * Resultado de uma verificação DNS.
 */
public class ResultadoVerificacao {
    private String url;
    private String ipRetornado;
    private boolean bloqueado;
    private String metodo;
    private boolean sucesso;
    private String mensagem;

    public ResultadoVerificacao(String url, String ipRetornado, boolean bloqueado, 
                                String metodo, boolean sucesso, String mensagem) {
        this.url = url;
        this.ipRetornado = ipRetornado;
        this.bloqueado = bloqueado;
        this.metodo = metodo;
        this.sucesso = sucesso;
        this.mensagem = mensagem;
    }

    public String getUrl() {
        return url;
    }

    public String getIpRetornado() {
        return ipRetornado;
    }

    public boolean isBloqueado() {
        return bloqueado;
    }

    public String getMetodo() {
        return metodo;
    }

    public boolean isSucesso() {
        return sucesso;
    }

    public String getMensagem() {
        return mensagem;
    }

    public static ResultadoVerificacao bloqueioConfirmado(String url, String ip, String metodo) {
        return new ResultadoVerificacao(
            url,
            ip,
            true,
            metodo,
            true,
            "✅ Bloqueio confirmado: " + url + " → " + ip
        );
    }

    public static ResultadoVerificacao naoFuncionou(String url, String ip, String metodo) {
        return new ResultadoVerificacao(
            url,
            ip,
            false,
            metodo,
            false,
            "❌ FALHOU: " + url + " não foi bloqueado (retornou " + ip + ")"
        );
    }

    public static ResultadoVerificacao permitido(String url, String ip, String metodo) {
        return new ResultadoVerificacao(
            url,
            ip,
            false,
            metodo,
            true,
            "✅ Site normal permitido: " + url + " → " + ip
        );
    }

    public static ResultadoVerificacao erro(String url, String metodo, String erro) {
        return new ResultadoVerificacao(
            url,
            "N/A",
            false,
            metodo,
            false,
            "⚠️ Erro ao testar " + url + ": " + erro
        );
    }
}

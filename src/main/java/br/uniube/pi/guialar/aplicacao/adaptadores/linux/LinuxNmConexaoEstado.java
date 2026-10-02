package br.uniube.pi.guialar.aplicacao.adaptadores.linux;

/**
 * Estado DNS de uma conexão NetworkManager antes da alteração (para reversão).
 */
public record LinuxNmConexaoEstado(
    String nome,
    String ipv4Dns,
    String ipv6Dns,
    String ipv4IgnoreAutoDns,
    String ipv6IgnoreAutoDns
) {
}

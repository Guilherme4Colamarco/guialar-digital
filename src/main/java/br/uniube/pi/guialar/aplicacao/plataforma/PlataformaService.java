package br.uniube.pi.guialar.aplicacao.plataforma;

import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.SistemaInfo;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.util.List;

/**
 * Abstração multiplataforma do GuiaLar Digital.
 *
 * Reúne as operações que a GUI e a CLI precisam, independentemente do sistema
 * operacional. Cada sistema fornece sua implementação (ex.: {@code LinuxPlataforma},
 * {@code WindowsPlataforma}), escolhida por {@link PlataformaFactory}.
 */
public interface PlataformaService {

    /** Descreve o sistema atual em linguagem amigável. */
    SistemaInfo descreverSistema();

    /** Detecta os navegadores instalados. */
    List<Navegador> detectarNavegadores();

    /** Indica se o programa está rodando com permissão de administrador. */
    boolean isAdministrador();

    /**
     * Instrução amigável de como executar como administrador neste sistema
     * (ex.: sudo no Linux, "Executar como administrador" no Windows).
     */
    String instrucoesAdmin();

    /** Ativa a proteção (troca o DNS do sistema para o Cloudflare Families). */
    ConfiguracaoDns protegerDns();

    /** Executa o smoke test que confere se a proteção está funcionando. */
    List<ResultadoVerificacao> verificarProtecao();

    /** Prepara o bloqueador de anúncios no navegador informado. */
    boolean prepararNavegador(Navegador navegador);
}

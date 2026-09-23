package br.uniube.pi.guialar.aplicacao.plataforma;

import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsBrowserDetector;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsDnsChanger;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsExtensionInstaller;
import br.uniube.pi.guialar.aplicacao.adaptadores.windows.WindowsVerificacaoDnsService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.SistemaInfo;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.util.List;

/**
 * Implementação da plataforma para Windows (10/11), usando PowerShell.
 */
public class WindowsPlataforma implements PlataformaService {

    private final WindowsBrowserDetector detector = new WindowsBrowserDetector();
    private final WindowsDnsChanger dnsChanger = new WindowsDnsChanger();
    private final WindowsExtensionInstaller instalador = new WindowsExtensionInstaller();
    private final WindowsVerificacaoDnsService verificacao = new WindowsVerificacaoDnsService();

    @Override
    public SistemaInfo descreverSistema() {
        String nome = System.getProperty("os.name");
        String versao = System.getProperty("os.version");
        boolean compativel = nome != null && nome.toLowerCase().contains("windows");
        String detalhe = (nome == null ? "" : nome) + (versao == null ? "" : " " + versao);
        return new SistemaInfo(TipoSistema.WINDOWS, nome == null ? "Windows" : nome, compativel, detalhe.trim());
    }

    @Override
    public List<Navegador> detectarNavegadores() {
        return detector.detectar();
    }

    @Override
    public boolean isAdministrador() {
        return AdminUtil.isAdminWindows();
    }

    @Override
    public String instrucoesAdmin() {
        return "feche o programa e abra novamente como administrador "
            + "(clique com o botão direito no programa e escolha \"Executar como administrador\")";
    }

    @Override
    public ConfiguracaoDns protegerDns() {
        return dnsChanger.configurar(ServidorDns.getPadrao());
    }

    @Override
    public List<ResultadoVerificacao> verificarProtecao() {
        List<ResultadoVerificacao> resultados = verificacao.verificar();
        new VerificacaoDnsService().exibirResumo(resultados);
        return resultados;
    }

    @Override
    public boolean prepararNavegador(Navegador navegador) {
        return instalador.instalar(navegador);
    }
}

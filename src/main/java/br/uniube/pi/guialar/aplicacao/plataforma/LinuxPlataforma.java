package br.uniube.pi.guialar.aplicacao.plataforma;

import br.uniube.pi.guialar.aplicacao.deteccao.DetectorDistroService;
import br.uniube.pi.guialar.aplicacao.deteccao.DetectorNavegadorService;
import br.uniube.pi.guialar.aplicacao.dns.ConfiguradorDnsService;
import br.uniube.pi.guialar.aplicacao.extensao.InstaladorExtensaoService;
import br.uniube.pi.guialar.aplicacao.verificacao.VerificacaoDnsService;
import br.uniube.pi.guialar.dominio.distro.InfoDistro;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.navegador.Navegador;
import br.uniube.pi.guialar.dominio.sistema.SistemaInfo;
import br.uniube.pi.guialar.dominio.sistema.TipoSistema;
import br.uniube.pi.guialar.dominio.verificacao.ResultadoVerificacao;

import java.util.List;

/**
 * Implementação da plataforma para Linux.
 *
 * Reaproveita integralmente os serviços Linux já existentes, preservando o
 * comportamento atual (Debian/Fedora/Arch).
 */
public class LinuxPlataforma implements PlataformaService {

    private final DetectorDistroService detectorDistro = new DetectorDistroService();
    private final DetectorNavegadorService detectorNavegador = new DetectorNavegadorService();
    private final ConfiguradorDnsService configuradorDns = new ConfiguradorDnsService();
    private final VerificacaoDnsService verificacaoDns = new VerificacaoDnsService();
    private final InstaladorExtensaoService instaladorExtensao = new InstaladorExtensaoService();

    private InfoDistro distroCache;

    private InfoDistro distro() {
        if (distroCache == null) {
            distroCache = detectorDistro.detectar();
        }
        return distroCache;
    }

    @Override
    public SistemaInfo descreverSistema() {
        InfoDistro d = distro();
        String detalhe = d.getTipo().getNomeExibicao()
            + (d.getGerenciadorRede() == null || d.getGerenciadorRede().isEmpty()
                ? "" : " / " + d.getGerenciadorRede());
        return new SistemaInfo(TipoSistema.LINUX, d.getNome(), d.isSuportada(), detalhe);
    }

    @Override
    public List<Navegador> detectarNavegadores() {
        return detectorNavegador.detectar();
    }

    @Override
    public boolean isAdministrador() {
        return AdminUtil.isRootLinux();
    }

    @Override
    public String instrucoesAdmin() {
        return "feche o programa e abra novamente como administrador "
            + "(no terminal: sudo java -jar guialar-digital.jar --gui)";
    }

    @Override
    public ConfiguracaoDns protegerDns() {
        return configuradorDns.configurar(distro());
    }

    @Override
    public List<ResultadoVerificacao> verificarProtecao() {
        List<ResultadoVerificacao> resultados = verificacaoDns.verificar();
        verificacaoDns.exibirResumo(resultados);
        return resultados;
    }

    @Override
    public boolean prepararNavegador(Navegador navegador) {
        return instaladorExtensao.instalar(navegador);
    }
}

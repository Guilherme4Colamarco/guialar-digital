package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

import br.uniube.pi.guialar.dominio.adaptadores.DnsChanger;
import br.uniube.pi.guialar.dominio.dns.ConfiguracaoDns;
import br.uniube.pi.guialar.dominio.dns.ServidorDns;

import java.time.Instant;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Implementação Windows do {@link DnsChanger}.
 *
 * - Leitura: Get-DnsClientServerAddress (sem admin)
 * - Escrita: Set-DnsClientServerAddress (com elevação UAC sob demanda)
 * - Reversão: ResetServerAddresses / restaura IPs do manifesto
 *
 * Em PC de laboratório sem admin: falha com mensagem clara e não altera o sistema.
 */
public class WindowsDnsChanger implements DnsChanger {

    public static final String METODO = "PowerShell Set-DnsClientServerAddress";

    private static final Pattern LINHA_ADAPTER = Pattern.compile(
        "^ALIAS=(.*)\\|INDEX=(\\d+)\\|IPV4=(.*)\\|IPV6=(.*)\\|DHCP=(.*)$",
        Pattern.MULTILINE);

    private final PowerShellExecutor executor;
    private final WindowsDnsManifestStore manifestoStore;
    private final WindowsElevationProbe elevation;

    public WindowsDnsChanger() {
        this(new ProcessPowerShellExecutor(), new WindowsDnsManifestStore());
    }

    public WindowsDnsChanger(PowerShellExecutor executor, WindowsDnsManifestStore manifestoStore) {
        this.executor = executor;
        this.manifestoStore = manifestoStore;
        this.elevation = new WindowsElevationProbe(executor);
    }

    /**
     * Lista adaptadores ativos e DNS atuais (somente leitura).
     */
    public List<WindowsDnsManifestStore.EstadoAdaptador> listarAdaptadoresAtivos() {
        PowerShellResult r = executor.executar(scriptListarAdaptadores());
        if (!r.isOk()) {
            return List.of();
        }
        return parseAdaptadores(r.getStdout());
    }

    public List<String> listarServidoresDnsAtuais() {
        List<String> servidores = new ArrayList<>();
        for (WindowsDnsManifestStore.EstadoAdaptador a : listarAdaptadoresAtivos()) {
            for (String ip : a.ipv4Anterior()) {
                if (!servidores.contains(ip)) {
                    servidores.add(ip);
                }
            }
            for (String ip : a.ipv6Anterior()) {
                if (!servidores.contains(ip)) {
                    servidores.add(ip);
                }
            }
        }
        return servidores;
    }

    /**
     * Motivo legível se a leitura de DNS falhou.
     */
    public String motivoFalhaLeituraDns() {
        PowerShellResult r = executor.executar(scriptListarAdaptadores());
        if (r.isIndisponivel()) {
            return "PowerShell indisponível: " + r.getStderr();
        }
        if (!r.isOk()) {
            String detalhe = r.getSaidaCombinada();
            if (detalhe.toLowerCase().contains("access") || detalhe.toLowerCase().contains("acesso")) {
                return "Leitura de DNS bloqueada pela política do sistema (GPO/filtro).";
            }
            return "Não foi possível ler DNS: " + (detalhe.isBlank() ? "erro desconhecido" : detalhe);
        }
        if (parseAdaptadores(r.getStdout()).isEmpty()) {
            return "Nenhum adaptador de rede ativo encontrado.";
        }
        return null;
    }

    @Override
    public ConfiguracaoDns configurar(ServidorDns servidor) {
        if (servidor == null) {
            servidor = ServidorDns.getPadrao();
        }

        PowerShellResult leitura = executor.executar(scriptListarAdaptadores());
        if (leitura.isIndisponivel()) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "PowerShell não disponível. O sistema não foi alterado. "
                    + "Diagnóstico e guias de navegador ainda funcionam.");
        }
        if (!leitura.isOk()) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "Não foi possível ler adaptadores de rede (possível GPO). "
                    + "O sistema não foi alterado. Motivo: " + truncar(leitura.getSaidaCombinada()));
        }

        List<WindowsDnsManifestStore.EstadoAdaptador> adaptadores = parseAdaptadores(leitura.getStdout());
        if (adaptadores.isEmpty()) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "Nenhum adaptador ativo encontrado. O sistema não foi alterado.");
        }

        Map<String, WindowsDnsManifestStore.EstadoAdaptador> mapa = new LinkedHashMap<>();
        for (WindowsDnsManifestStore.EstadoAdaptador a : adaptadores) {
            mapa.put(a.alias(), a);
        }

        boolean jaElevado = elevation.isProcessoElevado();
        String scriptAplicar = scriptAplicarDns(adaptadores, servidor);

        PowerShellResult escrita = executor.executar(scriptAplicar, !jaElevado);
        if (escrita.isElevacaoNegada()) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "Elevação UAC negada ou cancelada. Sem privilégios de administrador o DNS "
                    + "não pode ser alterado neste PC. O sistema permanece inalterado. "
                    + "Você ainda pode usar o diagnóstico e os guias de navegador.");
        }
        if (escrita.isIndisponivel()) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "Não foi possível executar PowerShell elevado. Sistema inalterado.");
        }
        if (!escrita.isOk()) {
            String msg = escrita.getSaidaCombinada().toLowerCase();
            if (msg.contains("access is denied") || msg.contains("acesso negado")
                    || msg.contains("unauthorized") || msg.contains("0x80070005")) {
                return ConfiguracaoDns.naoAplicado(METODO,
                    "DNS bloqueado por política (GPO) ou falta de permissão. "
                        + "Status: não aplicado. O sistema não foi alterado. "
                        + "Em PCs de laboratório use apenas o diagnóstico e os guias.");
            }
            return ConfiguracaoDns.naoAplicado(METODO,
                "Falha ao aplicar DNS. Sistema inalterado. Detalhe: " + truncar(escrita.getSaidaCombinada()));
        }

        try {
            WindowsDnsManifestStore.ManifestoDns manifesto =
                WindowsDnsManifestStore.ManifestoDns.criar(
                    METODO,
                    servidor.getPrimario() + "," + servidor.getSecundario(),
                    mapa);
            manifestoStore.salvar(manifesto);
        } catch (Exception e) {
            return ConfiguracaoDns.sucessoComAviso(servidor, METODO,
                "DNS aplicado, mas o manifesto de desfazer não pôde ser salvo em "
                    + manifestoStore.getArquivo() + ": " + e.getMessage());
        }

        return ConfiguracaoDns.sucesso(servidor, METODO);
    }

    @Override
    public boolean verificar() {
        ServidorDns alvo = ServidorDns.getPadrao();
        List<String> atuais = listarServidoresDnsAtuais();
        return atuais.contains(alvo.getPrimario()) || atuais.contains(alvo.getSecundario());
    }

    @Override
    public ConfiguracaoDns reverter() {
        try {
            WindowsDnsManifestStore.ManifestoDns manifesto = manifestoStore.carregar();
            if (manifesto == null || manifesto.adaptadores().isEmpty()) {
                return ConfiguracaoDns.naoAplicado(METODO,
                    "Nenhum manifesto encontrado em " + manifestoStore.getArquivo()
                        + ". Nada a desfazer (ou DNS nunca foi aplicado por este app).");
            }

            boolean jaElevado = elevation.isProcessoElevado();
            PowerShellResult r = executor.executar(scriptReverter(manifesto), !jaElevado);
            if (r.isElevacaoNegada()) {
                return ConfiguracaoDns.naoAplicado(METODO,
                    "Elevação UAC negada. Não foi possível desfazer. Sistema inalterado.");
            }
            if (!r.isOk()) {
                return ConfiguracaoDns.naoAplicado(METODO,
                    "Falha ao desfazer DNS: " + truncar(r.getSaidaCombinada())
                        + "\n" + getInstrucoesReversao());
            }
            manifestoStore.limpar();
            return new ConfiguracaoDns(
                ServidorDns.getPadrao(),
                true,
                METODO,
                "DNS revertido com sucesso a partir do manifesto ("
                    + manifesto.timestamp() + ").");
        } catch (Exception e) {
            return ConfiguracaoDns.naoAplicado(METODO,
                "Erro ao desfazer: " + e.getMessage());
        }
    }

    @Override
    public String getInstrucoesReversao() {
        return """
            Para desfazer o DNS no Windows manualmente:
            
            1. Abra PowerShell como Administrador
            2. Liste adaptadores: Get-NetAdapter
            3. Restaure DNS automático (DHCP), por exemplo:
               Set-DnsClientServerAddress -InterfaceAlias "Wi-Fi" -ResetServerAddresses
            4. Ou restaure IPs anteriores salvos no manifesto:
               %LOCALAPPDATA%\\GuiaLar\\dns-manifest.properties
            
            Alternativa pela interface:
            Configurações → Rede e Internet → Propriedades do adaptador → DNS → Automático
            """;
    }

    // --- scripts PowerShell ---

    public static String scriptListarAdaptadores() {
        return """
            $ErrorActionPreference = 'Continue'
            $adapters = Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' }
            if (-not $adapters) {
              # fallback: interfaces com DNS configurado
              $idxs = Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
                Where-Object { $_.ServerAddresses -and $_.ServerAddresses.Count -gt 0 } |
                Select-Object -ExpandProperty InterfaceIndex -Unique
              foreach ($i in $idxs) {
                $a = Get-NetAdapter -InterfaceIndex $i -ErrorAction SilentlyContinue
                if ($a) { $adapters += $a }
              }
            }
            foreach ($a in $adapters) {
              $v4 = Get-DnsClientServerAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
              $v6 = Get-DnsClientServerAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv6 -ErrorAction SilentlyContinue
              $ipv4 = @(); if ($v4 -and $v4.ServerAddresses) { $ipv4 = @($v4.ServerAddresses) }
              $ipv6 = @(); if ($v6 -and $v6.ServerAddresses) {
                $ipv6 = @($v6.ServerAddresses | Where-Object { $_ -and $_ -notmatch '^fe80' })
              }
              $dhcp = $true
              try {
                $cfg = Get-DnsClientServerAddress -InterfaceIndex $a.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue
                # se há endereços estáticos tipicamente ainda reportamos; Reset usa -ResetServerAddresses
              } catch {}
              $alias = $a.Name -replace '[\\|\\r\\n]',' '
              Write-Output ("ALIAS={0}|INDEX={1}|IPV4={2}|IPV6={3}|DHCP={4}" -f $alias, $a.ifIndex, ($ipv4 -join ','), ($ipv6 -join ','), $dhcp)
            }
            """;
    }

    public static String scriptAplicarDns(List<WindowsDnsManifestStore.EstadoAdaptador> adaptadores,
                                   ServidorDns servidor) {
        StringBuilder sb = new StringBuilder();
        sb.append("$ErrorActionPreference = 'Stop'\n");
        sb.append("$ok = 0\n");
        for (WindowsDnsManifestStore.EstadoAdaptador a : adaptadores) {
            sb.append(String.format("""
                try {
                  Set-DnsClientServerAddress -InterfaceIndex %d -ServerAddresses @('%s','%s')
                  try {
                    Set-DnsClientServerAddress -InterfaceIndex %d -AddressFamily IPv6 -ServerAddresses @('%s','%s')
                  } catch { }
                  $ok++
                } catch {
                  Write-Error $_.Exception.Message
                  exit 1
                }
                """,
                a.interfaceIndex(),
                servidor.getPrimario(), servidor.getSecundario(),
                a.interfaceIndex(),
                servidor.getPrimarioIpv6(), servidor.getSecundarioIpv6()));
        }
        sb.append("Write-Output \"APPLIED=$ok\"\n");
        sb.append("Write-Output \"TS=").append(Instant.now()).append("\"\n");
        return sb.toString();
    }

    public static String scriptReverter(WindowsDnsManifestStore.ManifestoDns manifesto) {
        StringBuilder sb = new StringBuilder();
        sb.append("$ErrorActionPreference = 'Stop'\n");
        for (WindowsDnsManifestStore.EstadoAdaptador a : manifesto.adaptadores().values()) {
            if (a.eraDhcp() || a.ipv4Anterior().isEmpty()) {
                sb.append(String.format("""
                    try {
                      Set-DnsClientServerAddress -InterfaceIndex %d -ResetServerAddresses
                    } catch {
                      Set-DnsClientServerAddress -InterfaceAlias '%s' -ResetServerAddresses
                    }
                    """, a.interfaceIndex(), a.alias().replace("'", "''")));
            } else {
                String ips = a.ipv4Anterior().stream()
                    .map(ip -> "'" + ip + "'")
                    .reduce((x, y) -> x + "," + y)
                    .orElse("");
                sb.append(String.format("""
                    try {
                      Set-DnsClientServerAddress -InterfaceIndex %d -ServerAddresses @(%s)
                    } catch {
                      Set-DnsClientServerAddress -InterfaceAlias '%s' -ServerAddresses @(%s)
                    }
                    """, a.interfaceIndex(), ips, a.alias().replace("'", "''"), ips));
            }
        }
        sb.append("Write-Output 'REVERTED'\n");
        return sb.toString();
    }

    public static List<WindowsDnsManifestStore.EstadoAdaptador> parseAdaptadores(String stdout) {
        List<WindowsDnsManifestStore.EstadoAdaptador> lista = new ArrayList<>();
        if (stdout == null || stdout.isBlank()) {
            return lista;
        }
        Matcher m = LINHA_ADAPTER.matcher(stdout);
        while (m.find()) {
            String alias = m.group(1).trim();
            int index = Integer.parseInt(m.group(2));
            List<String> ipv4 = splitIps(m.group(3));
            List<String> ipv6 = splitIps(m.group(4));
            boolean dhcp = !"false".equalsIgnoreCase(m.group(5).trim());
            lista.add(new WindowsDnsManifestStore.EstadoAdaptador(alias, index, ipv4, ipv6, dhcp));
        }
        return lista;
    }

    private static List<String> splitIps(String csv) {
        List<String> ips = new ArrayList<>();
        if (csv == null || csv.isBlank()) {
            return ips;
        }
        for (String p : csv.split(",")) {
            String t = p.trim();
            if (!t.isEmpty()) {
                ips.add(t);
            }
        }
        return ips;
    }

    private static String truncar(String s) {
        if (s == null) {
            return "";
        }
        String t = s.replaceAll("\\s+", " ").trim();
        return t.length() > 280 ? t.substring(0, 277) + "..." : t;
    }
}

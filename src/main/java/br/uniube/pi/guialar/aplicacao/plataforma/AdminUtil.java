package br.uniube.pi.guialar.aplicacao.plataforma;

/**
 * Verificação de privilégios administrativos por sistema operacional.
 */
public final class AdminUtil {

    private AdminUtil() {
    }

    /** No Linux: verdadeiro se o processo roda como root (uid 0). */
    public static boolean isRootLinux() {
        if ("root".equals(System.getProperty("user.name"))) {
            return true;
        }
        ProcessoUtil.Resultado r = ProcessoUtil.executar("id", "-u");
        return r.ok() && "0".equals(r.getSaida().trim());
    }

    /** No Windows: verdadeiro se o processo está elevado (grupo Administradores). */
    public static boolean isAdminWindows() {
        ProcessoUtil.Resultado r = ProcessoUtil.powershell(
            "([Security.Principal.WindowsPrincipal]"
                + "[Security.Principal.WindowsIdentity]::GetCurrent())"
                + ".IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)");
        return r.ok() && r.getSaida().trim().toLowerCase().contains("true");
    }
}

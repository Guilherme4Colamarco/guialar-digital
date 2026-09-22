package br.uniube.pi.guialar.aplicacao.adaptadores.windows;

/**
 * Consulta elevação / capacidade de elevar no Windows via PowerShell.
 * Em não-Windows ou quando a consulta falha, assume não elevado.
 */
public class WindowsElevationProbe {

    private final PowerShellExecutor executor;

    public WindowsElevationProbe(PowerShellExecutor executor) {
        this.executor = executor;
    }

    public boolean isProcessoElevado() {
        PowerShellResult r = executor.executar("""
            $id = [Security.Principal.WindowsIdentity]::GetCurrent()
            $p = New-Object Security.Principal.WindowsPrincipal($id)
            if ($p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { 'ELEVATED' } else { 'USER' }
            """);
        if (!r.isOk()) {
            return false;
        }
        return r.getStdout().toUpperCase().contains("ELEVATED");
    }

    /**
     * Em contas de laboratório o UAC pode existir mas GPO bloquear elevação.
     * Best-effort: se o processo já é admin, true; senão, assume que UAC pode
     * ser solicitado (a negação só aparece na aplicação do DNS).
     */
    public boolean podeSolicitarElevacao() {
        if (isProcessoElevado()) {
            return true;
        }
        PowerShellResult r = executor.executar("""
            try {
              $key = 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System'
              $enable = (Get-ItemProperty -Path $key -Name EnableLUA -ErrorAction SilentlyContinue).EnableLUA
              if ($null -eq $enable -or $enable -eq 1) { 'UAC_OK' } else { 'UAC_OFF' }
            } catch { 'UAC_UNKNOWN' }
            """);
        if (r.isIndisponivel()) {
            return false;
        }
        String out = r.getStdout().toUpperCase();
        if (out.contains("UAC_OFF")) {
            return false;
        }
        return true;
    }
}

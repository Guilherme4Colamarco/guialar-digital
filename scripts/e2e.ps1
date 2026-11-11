# E2E headless pela CLI (ADR-029, camada 4) para Windows. Sem GUI, sem clique.
# Uso (PowerShell ELEVADO, como administrador):  .\e2e.ps1 -Bin C:\caminho\guialar.exe
# Roteiro fixo: status -> apply --mode family -> DNS pelo SO -> sonda
#   -> apply de novo (idempotente) -> revert -> DNS igual ao de antes,
#   state.json ausente, sonda volta a resolver.
# Uma linha PASS/FAIL por passo. Exit 0 so se todos passarem; 1 se algum falhar;
# 2 se o shell nao estiver elevado (recusa, sem alterar nada) ou o uso for invalido.
# ATENCAO: altera o DNS do computador. Rode so em VM isolada, nunca no host.
param([Parameter(Mandatory = $true)][string]$Bin)

$ErrorActionPreference = 'Continue'
$Probe = 'nudity.testcategory.com'
$FamilyA = '1.1.1.3'
$FamilyB = '1.0.0.3'
$State = Join-Path $env:LOCALAPPDATA 'GuiaLar\state.json'
$LogDir = Join-Path $env:TEMP ('guialar-e2e-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $LogDir -Force | Out-Null
$script:Failed = $false
$script:BaseDns = ''

function Pass($n, $msg) { Write-Output "PASS  [$n] $msg" }
function Fail($n, $msg) { Write-Output "FAIL  [$n] $msg"; $script:Failed = $true }
function Info($msg) { Write-Output "INFO  $msg" }

# Recusa sem elevacao: nao tenta UAC nem finge resultado.
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Output 'FAIL  [0] shell NAO elevado: este script recusa rodar. Abra o PowerShell como administrador (ou use SSH com sessao elevada). Nada foi alterado.'
    exit 2
}
if (-not (Test-Path $Bin)) {
    Write-Output "FAIL  [0] binario nao encontrado: $Bin"
    exit 2
}

function Get-OsDns {
    $ips = Get-DnsClientServerAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object { $_.ServerAddresses } |
        ForEach-Object { $_.ServerAddresses } |
        Where-Object { $_ -notlike '127.*' } |
        Sort-Object -Unique
    return ($ips -join ',')
}

function Get-Probe {
    Clear-DnsClientCache -ErrorAction SilentlyContinue
    $ips = Resolve-DnsName -Name $Probe -Type A -DnsOnly -ErrorAction SilentlyContinue |
        Where-Object { $_.Type -eq 'A' } |
        ForEach-Object { $_.IPAddress } |
        Sort-Object -Unique
    return ($ips -join ',')
}

function Test-DnsIsFamily {
    $d = ',' + (Get-OsDns) + ','
    return ($d.Contains(",$FamilyA,") -and $d.Contains(",$FamilyB,"))
}
function Test-DnsIsBaseline { return ($script:BaseDns -ne '' -and (Get-OsDns) -eq $script:BaseDns) }
function Test-ProbeBlocked { return ((Get-Probe) -eq '0.0.0.0') }
function Test-ProbeResolves {
    $p = Get-Probe
    return ($p -ne '' -and $p -ne '0.0.0.0')
}

function Wait-Until([scriptblock]$Cond, [int]$Seconds) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    while ($true) {
        if (& $Cond) { return $true }
        if ((Get-Date) -ge $deadline) { return $false }
        Start-Sleep -Seconds 2
    }
}

# Roda a CLI, espera o fim e devolve o exit code. A saida vai para arquivo e para o console
# (Write-Host), sem entrar no retorno da funcao. O guialar.exe usa o subsistema "windows":
# com `&` o PowerShell nao espera o processo nem preenche $LASTEXITCODE, por isso Start-Process -Wait.
function Invoke-Cli([string]$Name, [string[]]$CliArgs) {
    $out = Join-Path $LogDir "$Name.out"
    $err = Join-Path $LogDir "$Name.err"
    $p = Start-Process -FilePath $Bin -ArgumentList $CliArgs -Wait -PassThru -NoNewWindow `
        -RedirectStandardOutput $out -RedirectStandardError $err
    $code = [int]$p.ExitCode
    Set-Content -Path (Join-Path $LogDir "$Name.rc") -Value $code
    Get-Content -Path $out -Encoding UTF8 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "      $_" }
    Get-Content -Path $err -Encoding UTF8 -ErrorAction SilentlyContinue | ForEach-Object { Write-Host "      [stderr] $_" }
    return $code
}

function Get-Rc([string]$Name) {
    $f = Join-Path $LogDir "$Name.rc"
    if (Test-Path $f) { return (Get-Content $f) } else { return '?' }
}

Info "logs em $LogDir"
Info "binario: $Bin ($((Get-FileHash $Bin -Algorithm SHA256).Hash.Substring(0, 16))...)"

# Precondicoes: o teste so faz sentido partindo de um computador sem protecao.
$script:BaseDns = Get-OsDns
Info "DNS inicial (SO): $(if ($script:BaseDns) { $script:BaseDns } else { '<vazio>' })"
Info "sonda inicial: $(Get-Probe)"
if (Test-Path $State) {
    Fail 0 "precondicao: state.json ja existe antes do teste; use uma VM limpa (ou revert antes)"
    exit 1
}
if ($script:BaseDns -eq '') {
    Fail 0 'precondicao: nao consegui ler o DNS inicial pelo SO'
    exit 1
}
if (Test-ProbeBlocked) {
    Fail 0 'precondicao: a sonda ja retorna 0.0.0.0 antes do teste (filtro ja ativo)'
    exit 1
}
# Trava de operacao (%TEMP%\guialar-op.lock) de uma execucao anterior interrompida: no Windows
# ela so expira por idade (>120 s). Espera ate 130 s antes de comecar.
$lockFile = Join-Path $env:TEMP 'guialar-op.lock'
function Test-LockFree {
    if (-not (Test-Path $lockFile)) { return $true }
    $age = ((Get-Date) - (Get-Item $lockFile).LastWriteTime).TotalSeconds
    return ($age -gt 120)
}
if (-not (Test-LockFree)) {
    Info 'trava guialar-op.lock recente; esperando expirar (ate 150 s)'
    if (-not (Wait-Until { Test-LockFree } 150)) {
        Fail 0 'precondicao: trava guialar-op.lock continua recente apos 150 s'
        exit 1
    }
}

try {
    # 1. status
    if ((Invoke-Cli '01-status' @('status')) -eq 0) { Pass 1 'status: exit 0' }
    else { Fail 1 "status: exit $(Get-Rc '01-status')" }

    # 2. apply --mode family
    if ((Invoke-Cli '02-apply' @('apply', '--mode', 'family')) -eq 0) { Pass 2 'apply --mode family: exit 0' }
    else { Fail 2 "apply --mode family: exit $(Get-Rc '02-apply')" }

    # 3. DNS pelo SO
    if (Wait-Until { Test-DnsIsFamily } 30) { Pass 3 "DNS do SO = $(Get-OsDns) (contem $FamilyA e $FamilyB)" }
    else { Fail 3 "DNS do SO = $(Get-OsDns); esperado conter $FamilyA e $FamilyB" }

    # 4. sonda bloqueada
    if (Wait-Until { Test-ProbeBlocked } 30) { Pass 4 "sonda $Probe = 0.0.0.0" }
    else { Fail 4 "sonda $Probe = $(Get-Probe); esperado 0.0.0.0" }

    # 5. apply de novo (idempotente: manifesto inalterado, DNS continua o da familia)
    $shaBefore = if (Test-Path $State) { (Get-FileHash $State -Algorithm SHA256).Hash } else { '' }
    if ((Invoke-Cli '05-apply-again' @('apply', '--mode', 'family')) -eq 0) {
        $shaAfter = if (Test-Path $State) { (Get-FileHash $State -Algorithm SHA256).Hash } else { '' }
        if ($shaBefore -ne '' -and $shaBefore -eq $shaAfter -and (Wait-Until { Test-DnsIsFamily } 15)) {
            Pass 5 "apply de novo: exit 0, state.json inalterado, DNS = $(Get-OsDns)"
        } else {
            Fail 5 "apply de novo: state.json antes=$shaBefore depois=$shaAfter; DNS = $(Get-OsDns)"
        }
    } else { Fail 5 "apply de novo: exit $(Get-Rc '05-apply-again')" }

    # 6. revert
    if ((Invoke-Cli '06-revert' @('revert')) -eq 0) { Pass 6 'revert: exit 0' }
    else { Fail 6 "revert: exit $(Get-Rc '06-revert')" }

    # 7. DNS igual ao de antes
    if (Wait-Until { Test-DnsIsBaseline } 45) { Pass 7 "DNS do SO igual ao inicial: $(Get-OsDns)" }
    else { Fail 7 "DNS do SO = $(Get-OsDns); esperado o inicial: $($script:BaseDns)" }

    # 8. state.json ausente
    if (-not (Test-Path $State)) { Pass 8 'state.json ausente apos revert' }
    else { Fail 8 "state.json ainda existe: $State" }

    # 9. sonda volta a resolver
    if (Wait-Until { Test-ProbeResolves } 30) { Pass 9 "sonda $Probe resolve de novo: $(Get-Probe)" }
    else { Fail 9 "sonda $Probe = '$(Get-Probe)'; esperado um IP real (nao vazio, nao 0.0.0.0)" }
}
finally {
    # Limpeza: se algum passo deixou o filtro ativo, desfaz para nao deixar o DNS filtrado.
    if (Test-Path $State) {
        Info 'limpeza: state.json ficou; rodando revert'
        & $Bin revert *> (Join-Path $LogDir 'cleanup-revert.out')
    }
}

if (-not $script:Failed) {
    Write-Output 'RESULTADO: PASS (9/9 passos)'
    exit 0
}
Write-Output "RESULTADO: FAIL (ver linhas FAIL acima; logs em $LogDir)"
exit 1

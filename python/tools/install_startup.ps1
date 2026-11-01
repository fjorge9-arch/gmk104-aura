# Instala o gmk104-aura na pasta Inicializar do usuário (shell:startup). NÃO precisa de administrador.
# LEGADO: o binário Rust (gmk104-aura.exe) já cuida do início automático pelo menu da bandeja.
# Usa o binário Rust (target\release\gmk104-aura.exe na raiz do repositório) se existir; senão, pythonw run.py.
# Cria um atalho que roda o programa sem janela a cada logon, e inicia o programa agora.
# Para remover: tools\uninstall_startup.ps1
param([switch]$Python)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path (Split-Path -Parent $root) "target\release\gmk104-aura.exe"

if ((Test-Path $exe) -and -not $Python) {
    $target = $exe; $args = ""; $workdir = Split-Path -Parent $exe
} else {
    $target = Join-Path (Split-Path -Parent (Get-Command python).Source) "pythonw.exe"
    if (-not (Test-Path $target)) { throw "pythonw.exe não encontrado ao lado de python.exe" }
    $args = "`"$root\run.py`""; $workdir = $root
}

$startup = [Environment]::GetFolderPath("Startup")
$lnk = Join-Path $startup "gmk104-aura.lnk"
$ws = New-Object -ComObject WScript.Shell
$sc = $ws.CreateShortcut($lnk)
$sc.TargetPath = $target
$sc.Arguments = $args
$sc.WorkingDirectory = $workdir
$sc.WindowStyle = 7
$sc.Description = "Teclado GMK104 segue a cor do ASUS Aura Sync"
$sc.Save()

# encerra instâncias anteriores (Python ou Rust) e inicia agora
Get-CimInstance Win32_Process -Filter "Name like 'python%'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*gmk104-aura*run.py*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Get-Process -Name "gmk104-aura" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
if ($args) { Start-Process -FilePath $target -ArgumentList $args -WorkingDirectory $workdir }
else { Start-Process -FilePath $target -WorkingDirectory $workdir }
Write-Host "Atalho criado em $lnk -> $target. Programa iniciado. Log em $env:LOCALAPPDATA\gmk104-aura\gmk104-aura.log"

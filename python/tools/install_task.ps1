# Cria a tarefa agendada "gmk104-aura": inicia no logon do usuário atual, sem janela (pythonw),
# SEM privilégios elevados. A cor do modo Smart vem do log público do LightingService, então não é
# preciso administrador. (Nesta máquina, tarefas com "nível mais alto" travavam ao iniciar.)
# Pode ser executado em um PowerShell comum; use -Elevated para registrar com RunLevel Highest
# (só se quiser a leitura direta da temperatura via serviço ASUS como segunda fonte).
param([switch]$Elevated)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$pyw = Join-Path (Split-Path -Parent (Get-Command python).Source) "pythonw.exe"
if (-not (Test-Path $pyw)) { throw "pythonw.exe não encontrado ao lado de python.exe" }

$user = "$env:USERDOMAIN\$env:USERNAME"
$action = New-ScheduledTaskAction -Execute $pyw -Argument "`"$root\run.py`"" -WorkingDirectory $root
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $user
$trigger.Delay = "PT20S"   # espera o LightingService e o teclado subirem
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) `
    -MultipleInstances IgnoreNew -Priority 4
$level = if ($Elevated) { "Highest" } else { "Limited" }
$principal = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel $level

# encerra instância anterior (se houver) antes de recriar
Stop-ScheduledTask -TaskName "gmk104-aura" -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name like 'python%'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*gmk104-aura*run.py*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }

Register-ScheduledTask -TaskName "gmk104-aura" -Action $action -Trigger $trigger -Settings $settings `
    -Principal $principal -Description "Teclado GMK104 segue a cor do ASUS Aura Sync" -Force | Out-Null
Start-ScheduledTask -TaskName "gmk104-aura"
Write-Host "Tarefa 'gmk104-aura' criada ($level) e iniciada. Log em $env:LOCALAPPDATA\gmk104-aura\gmk104-aura.log"

# Remove a tarefa agendada "gmk104-aura" e encerra o processo em execução. Execute como administrador.
Stop-ScheduledTask -TaskName "gmk104-aura" -ErrorAction SilentlyContinue
Unregister-ScheduledTask -TaskName "gmk104-aura" -Confirm:$false -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name like 'python%'" |
    Where-Object { $_.CommandLine -like "*gmk104-aura*run.py*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Write-Host "Tarefa removida."

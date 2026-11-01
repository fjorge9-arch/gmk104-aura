# Remove o atalho da pasta Inicializar e encerra o programa (Python ou Rust). Não precisa de administrador.
$lnk = Join-Path ([Environment]::GetFolderPath("Startup")) "gmk104-aura.lnk"
Remove-Item $lnk -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -Filter "Name like 'python%'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*gmk104-aura*run.py*" } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Get-Process -Name "gmk104-aura" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "Atalho removido e programa encerrado."

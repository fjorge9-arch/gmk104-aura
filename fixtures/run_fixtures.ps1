# Reproduz cada efeito capturado em fixtures/aura-scripts no teclado, por alguns segundos cada,
# apontando o programa (GMK104_LS_DIR) para uma cópia temporária com LastProfile.xml + script\LastScript.xml.
# Uso: powershell -ExecutionPolicy Bypass -File fixtures\run_fixtures.ps1 [-Seconds 6] [-Only breathing]
param([int]$Seconds = 6, [string]$Only = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\release\gmk104-aura.exe"
$fx = Join-Path $PSScriptRoot "aura-scripts"
$tmp = Join-Path $env:TEMP "gmk104-fixture"
$log = Join-Path $env:LOCALAPPDATA "gmk104-aura\gmk104-aura.log"

$cases = @(
    @{ name = "static";        profile = "profile_static.xml" },
    @{ name = "breathing";     profile = "profile_static.xml" },
    @{ name = "color_cycle";   profile = "profile_static.xml" },
    @{ name = "rainbow";       profile = "profile_static.xml" },
    @{ name = "strobing";      profile = "profile_static.xml" },
    @{ name = "starry_night";  profile = "profile_static.xml" },
    @{ name = "music";         profile = "profile_static.xml" },
    @{ name = "adaptive_color";profile = "profile_static.xml" },
    @{ name = "halloween";     profile = "profile_static.xml" },
    @{ name = "off";           profile = "profile_off.xml"; script = "static.xml" },
    @{ name = "smart";         profile = "profile_smart.xml" }
)
foreach ($c in $cases) {
    if ($Only -and $c.name -ne $Only) { continue }
    $script = if ($c.script) { $c.script } else { "$($c.name).xml" }
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path (Join-Path $tmp "script") -Force | Out-Null
    Copy-Item (Join-Path $fx $c.profile) (Join-Path $tmp "LastProfile.xml")
    Copy-Item (Join-Path $fx $script) (Join-Path $tmp "script\LastScript.xml")
    $env:GMK104_LS_DIR = $tmp
    $before = if (Test-Path $log) { (Get-Content $log).Count } else { 0 }
    Write-Host "=== $($c.name)"
    # o exe é um app sem console: & não espera; Start-Process -Wait sim
    Start-Process -FilePath $exe -ArgumentList "--no-tray", "--duration", $Seconds -Wait
    Get-Content $log | Select-Object -Skip $before | Where-Object { $_ -match "perfil Aura|teclado <-|WARNING|ERROR" } |
        ForEach-Object { "   " + $_.Substring(24) }
}
Remove-Item Env:GMK104_LS_DIR -ErrorAction SilentlyContinue

<#
.SYNOPSIS
    Builds RustyWeatherPill as a single-file portable exe for Windows 11.

.DESCRIPTION
    Produces a standalone executable that needs no installer, no Visual C++
    Redistributable and no files next to it:

      * the MSVC CRT/UCRT is linked statically (see .cargo/config.toml),
      * the tray icon is embedded in the exe resource,
      * settings/cache live in %LOCALAPPDATA%\WeatherPill.

    The script rebuilds the release exe, verifies that it imports only system
    DLLs, and copies it to <OutDir>\RustyWeatherPill.exe (optionally zipped).

.PARAMETER OutDir
    Output directory for the portable exe. Default: dist

.PARAMETER Zip
    Also produce <OutDir>\RustyWeatherPill-portable.zip.

.PARAMETER Run
    Launch the portable exe for 5 seconds as a smoke test, then stop it.

.EXAMPLE
    .\make-portable.ps1

.EXAMPLE
    .\make-portable.ps1 -OutDir C:\Portable -Zip -Run
#>
[CmdletBinding()]
param(
    [string]$OutDir = "dist",
    [switch]$Zip,
    [switch]$Run
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSCommandPath
$exeName = "RustyWeatherPill.exe"

function Get-PeImports {
    param([Parameter(Mandatory)][string]$Path)
    $b = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Path))
    if ($b.Length -lt 0x40) { throw "not a PE file" }
    $lfanew = [BitConverter]::ToInt32($b, 0x3C)
    if ($b[$lfanew] -ne 0x50 -or $b[$lfanew + 1] -ne 0x45) { throw "missing PE signature" }
    $numSections = [BitConverter]::ToUInt16($b, $lfanew + 6)
    $sizeOpt = [BitConverter]::ToUInt16($b, $lfanew + 20)
    $opt = $lfanew + 24
    $magic = [BitConverter]::ToUInt16($b, $opt)
    $ddOff = $opt + $(if ($magic -eq 0x20b) { 112 } else { 96 })
    $importRva = [BitConverter]::ToUInt32($b, $ddOff + 8)
    if ($importRva -eq 0) { return @() }
    $sect = $opt + $sizeOpt
    $sections = @()
    for ($i = 0; $i -lt $numSections; $i++) {
        $o = $sect + 40 * $i
        $sections += [pscustomobject]@{
            VA   = [BitConverter]::ToUInt32($b, $o + 12)
            VLen = [Math]::Max([BitConverter]::ToUInt32($b, $o + 8), [BitConverter]::ToUInt32($b, $o + 16))
            Raw  = [BitConverter]::ToUInt32($b, $o + 20)
        }
    }
    function RvaToOff([uint32]$rva) {
        foreach ($s in $sections) {
            if ($rva -ge $s.VA -and $rva -lt ($s.VA + $s.VLen)) { return [int]($s.Raw + ($rva - $s.VA)) }
        }
        throw ("RVA 0x{0:X} not mapped" -f $rva)
    }
    $names = New-Object System.Collections.Generic.List[string]
    $desc = RvaToOff $importRva
    while ($true) {
        $nameRva = [BitConverter]::ToUInt32($b, $desc + 12)
        if ($nameRva -eq 0) { break }
        $off = RvaToOff $nameRva
        $sb = New-Object System.Text.StringBuilder
        while ($b[$off] -ne 0) { [void]$sb.Append([char]$b[$off]); $off++ }
        $names.Add($sb.ToString())
        $desc += 20
    }
    return $names.ToArray()
}

function Get-PeMachine {
    param([Parameter(Mandatory)][string]$Path)
    $b = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Path))
    $lfanew = [BitConverter]::ToInt32($b, 0x3C)
    $machine = [BitConverter]::ToUInt16($b, $lfanew + 4)
    switch ($machine) {
        0x8664 { "x64" }
        0xAA64 { "arm64" }
        0x014c { "x86" }
        default { ("0x{0:X4}" -f $machine) }
    }
}

Push-Location $root
try {
    Write-Host "== RustyWeatherPill portable build ==" -ForegroundColor Cyan

    # --- locate cargo -------------------------------------------------------
    $cargoCmd = Get-Command cargo.exe -ErrorAction SilentlyContinue
    if ($cargoCmd) {
        $cargo = $cargoCmd.Source
    } else {
        $cargo = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
        if (-not (Test-Path -LiteralPath $cargo)) {
            throw "cargo.exe not found. Install Rust (rustup) or add %USERPROFILE%\.cargo\bin to PATH."
        }
    }

    # --- ensure the portable toolchain config is in place -------------------
    $cfg = Join-Path $root ".cargo\config.toml"
    if (-not (Test-Path -LiteralPath $cfg) -or
        -not (Select-String -LiteralPath $cfg -Pattern "crt-static" -Quiet)) {
        throw "Missing .cargo\config.toml with 'target-feature=+crt-static' - refusing to build a non-portable exe."
    }

    # --- stop running copies (they would lock the exe the linker writes) ---
    foreach ($name in @("rusty_weather_pill", "RustyWeatherPill")) {
        $running = @(Get-Process -Name $name -ErrorAction SilentlyContinue)
        if ($running.Count -gt 0) {
            $running | Stop-Process -Force
            Write-Host ("Stopped {0} running {1} process(es)." -f $running.Count, $name)
            Start-Sleep -Milliseconds 500
        }
    }

    # --- remove the previous artifact so a stale exe can never ship ---------
    $built = Join-Path $root "target\release\rusty_weather_pill.exe"
    if (Test-Path -LiteralPath $built) {
        Remove-Item -LiteralPath $built -Force
        Write-Host "Removed previous release exe."
    }

    # --- build --------------------------------------------------------------
    Write-Host "Building (release, static CRT)..." -ForegroundColor Cyan
    & $cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    if (-not (Test-Path -LiteralPath $built)) { throw "build finished but $built is missing" }

    # --- verify portability -------------------------------------------------
    $imports = @(Get-PeImports $built)
    $bad = @($imports | Where-Object { $_ -match '^(vcruntime|msvcp|concrt|ucrtbase|api-ms-win-crt-)' })
    if ($bad.Count -gt 0) {
        throw ("Exe is NOT portable - still depends on the VC++ runtime: " + ($bad -join ", "))
    }
    $arch = Get-PeMachine $built

    # --- publish ------------------------------------------------------------
    $outPath = if ([IO.Path]::IsPathRooted($OutDir)) { $OutDir } else { Join-Path $root $OutDir }
    New-Item -ItemType Directory -Force -Path $outPath | Out-Null
    $dest = Join-Path $outPath $exeName
    Copy-Item -LiteralPath $built -Destination $dest -Force

    $hash = (Get-FileHash -LiteralPath $dest -Algorithm SHA256).Hash
    $sizeMb = [Math]::Round((Get-Item -LiteralPath $dest).Length / 1MB, 2)

    Write-Host ""
    Write-Host "Portable exe: $dest" -ForegroundColor Green
    Write-Host "  architecture : $arch"
    Write-Host "  size         : $sizeMb MB"
    Write-Host "  sha256       : $hash"
    Write-Host ("  imports      : " + (($imports | Sort-Object -Unique) -join ", "))
    Write-Host "  no VC++ redist / UCRT DLL dependency; icon embedded; config in %LOCALAPPDATA%\WeatherPill"

    if ($Zip) {
        $zipPath = Join-Path $outPath "RustyWeatherPill-portable.zip"
        if (Test-Path -LiteralPath $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
        Compress-Archive -LiteralPath $dest -DestinationPath $zipPath -CompressionLevel Optimal
        Write-Host "Zip: $zipPath" -ForegroundColor Green
    }

    if ($Run) {
        Write-Host "Smoke test: launching for 5 s..." -ForegroundColor Cyan
        $proc = Start-Process -FilePath $dest -PassThru
        Start-Sleep -Seconds 5
        if ($proc.HasExited) { throw "portable exe exited during smoke test (code $($proc.ExitCode))" }
        Stop-Process -Id $proc.Id -Force
        Write-Host "Smoke test passed: process stayed alive." -ForegroundColor Green
    }

    Write-Host ""
    Write-Host "Done." -ForegroundColor Green
    exit 0
}
catch {
    Write-Host ""
    Write-Host ("ERROR: " + $_.Exception.Message) -ForegroundColor Red
    exit 1
}
finally {
    Pop-Location
}

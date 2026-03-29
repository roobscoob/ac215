#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

$BinPath = Join-Path $HOME "proxy.exe"
$ConfigPath = Join-Path $HOME "proxy.toml"

if (-not (Test-Path $BinPath)) {
    Write-Error "Binary not found at $BinPath - run 'cargo build --release --bin proxy' first."
    exit 1
}

if (-not (Test-Path $ConfigPath)) {
    Write-Error "Config not found at $ConfigPath"
    exit 1
}

# Install the proxy service.
New-Service -Name "Ac215Proxy" `
    -BinaryPathName ('"' + $BinPath + '" --service "' + $ConfigPath + '"') `
    -DisplayName "AC-215 Panel Proxy" `
    -Description "Proxies traffic between the AC-215 panel and AxTraxNG Server" `
    -StartupType Automatic

Write-Host "Installed Ac215Proxy service."

# Add Ac215Proxy as a dependency of AxTraxNG Server, preserving existing dependencies.
$Existing = (Get-Service -Name "AxTraxNG Server").ServicesDependedOn | ForEach-Object { $_.Name }
$Dependencies = @($Existing) + @("Ac215Proxy") | Select-Object -Unique
$DepString = ($Dependencies -join "/")
sc.exe config "AxTraxNG Server" depend= $DepString | Out-Null

Write-Host "AxTraxNG Server now depends on: $($Dependencies -join ', ')"

# Start the service.
Start-Service -Name "Ac215Proxy"
Restart-Service -Name "AxTraxNG Server"
Write-Host "Ac215Proxy started."

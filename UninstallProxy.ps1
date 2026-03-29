#Requires -RunAsAdministrator

$ErrorActionPreference = "Stop"

$BinPath = Join-Path $HOME "proxy.exe"
$ConfigPath = Join-Path $HOME "proxy.toml"

# Remove Ac215Proxy from AxTraxNG Server's dependencies.
$Existing = (Get-Service -Name "AxTraxNG Server").ServicesDependedOn | ForEach-Object { $_.Name }
$Dependencies = @($Existing) | Where-Object { $_ -ne "Ac215Proxy" }

if ($Dependencies.Count -gt 0) {
    $DepString = ($Dependencies -join "/")
    sc.exe config "AxTraxNG Server" depend= $DepString | Out-Null
} else {
    sc.exe config "AxTraxNG Server" depend= "" | Out-Null
}

Write-Host "Removed Ac215Proxy from AxTraxNG Server dependencies."

# Stop and remove the proxy service.
$Service = Get-Service -Name "Ac215Proxy" -ErrorAction SilentlyContinue
if ($Service) {
    Stop-Service "AxTraxNG Server"
    if ($Service.Status -ne "Stopped") {
        Stop-Service -Name "Ac215Proxy"
        Write-Host "Stopped Ac215Proxy."
    }
    sc.exe delete "Ac215Proxy" | Out-Null
    Write-Host "Removed Ac215Proxy service."
} else {
    Write-Host "Ac215Proxy service not found, nothing to remove."
}

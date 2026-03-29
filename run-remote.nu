def main [
    --log-level: string = "trace"
    --install
    --uninstall
] {
    scp InstallProxy.ps1 rc-security:~/InstallProxy.ps1
    scp UninstallProxy.ps1 rc-security:~/UninstallProxy.ps1

    if $uninstall {
        ssh -t rc-security "powershell -ExecutionPolicy Bypass -File UninstallProxy.ps1"
        return
    }

    cargo build --bin proxy
    scp target/debug/proxy.exe rc-security:~/proxy.exe

    if $install {
        ssh -t rc-security "powershell -ExecutionPolicy Bypass -File InstallProxy.ps1"
    } else {
        ssh -t rc-security $"set RUST_LOG=($log_level)&& proxy.exe"
    }
}

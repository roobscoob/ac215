def main [--log-level: string = "trace"] {
    cargo build --bin proxy
    scp target/debug/proxy.exe rc-security:~/proxy.exe
    # ssh -t rc-security $"set RUST_LOG=ac215=($log_level)&& proxy.exe"
    ssh -t rc-security $"set RUST_LOG=($log_level)&& proxy.exe"
}
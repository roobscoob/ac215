cargo build --bin proxy
scp target/debug/proxy.exe rc-security:~/proxy.exe
ssh rc-security proxy.exe
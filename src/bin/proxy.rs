use std::net::SocketAddr;

use ac215::crypto::Cipher;
use ac215::packet::header::ChecksumMode;
use ac215::proxy::Proxy;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let listen_addr: SocketAddr = args
        .get(1)
        .unwrap_or(&"0.0.0.0:2000".to_string())
        .parse()
        .expect("invalid listen address");

    let panel_addr: SocketAddr = args
        .get(2)
        .unwrap_or(&"192.168.1.136:1818".to_string())
        .parse()
        .expect("invalid panel address");

    println!("AC-215 Proxy");
    println!("  Listen: {listen_addr} (server connects here)");
    println!("  Panel:  {panel_addr} (real panel)");
    println!();

    let proxy = Proxy::start(listen_addr, panel_addr, Cipher::new(), ChecksumMode::Auto)
        .await
        .expect("failed to start proxy");

    proxy.run().await;
}

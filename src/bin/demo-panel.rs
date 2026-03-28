use std::net::SocketAddr;

use ac215::{
    crypto::Cipher,
    packet::{
        address::{Ac215Address, Ac215AddressMode},
        header::ChecksumMode,
        packets::{
            answer_firm_ver::AnswerFirmVer825Packet, request_firm_ver_825::RequestFirmVer825Packet,
        },
    },
    panel::Panel,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let listen_addr: SocketAddr = args
        .get(1)
        .unwrap_or(&"0.0.0.0:1918".to_string())
        .parse()
        .expect("invalid listen address");

    let panel_address = Ac215Address::panel_main_controller(Ac215AddressMode::Dual);

    println!("Starting simulated panel on {}...", listen_addr);

    let mut panel = Panel::listen(
        listen_addr,
        Cipher::new(),
        panel_address,
        ChecksumMode::Auto,
    )
    .await
    .expect("failed to start panel");

    println!("Panel ready, listening for frames...");

    while let Some(frame) = panel.recv().await {
        if let Some(Ok(v)) = frame.parse::<RequestFirmVer825Packet>() {
            println!("Received firmware version request");
        }

        println!(
            "[0x{:02X}] {:?} -> {:?} ({} bytes payload)",
            frame.header().command_id(),
            frame.header().source(),
            frame.header().destination(),
            frame.payload().len(),
        );
    }

    println!("Connection closed");
}

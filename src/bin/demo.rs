use std::net::SocketAddr;

use ac215::{
    crypto::Cipher,
    packet::{
        address::{Ac215Address, Ac215AddressMode},
        header::ChecksumMode,
        packets::{
            manual_output::ManualOutputPacket, request_firm_ver_825::RequestFirmVer825Packet,
        },
    },
    server::Server,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let addr: SocketAddr = args
        .get(1)
        .unwrap_or(&"192.168.1.136:1818".to_string())
        .parse()
        .expect("invalid address");

    println!("Connecting to panel at {}...", addr);

    let mut server = Server::connect(addr, Cipher::new(), ChecksumMode::Auto)
        .await
        .expect("failed to connect");

    println!("Connected. Requesting firmware version...");

    let reply = server
        .request(
            Ac215Address::panel_main_controller(Ac215AddressMode::Dual),
            RequestFirmVer825Packet,
            ChecksumMode::Auto,
        )
        .await
        .expect("request failed");

    println!("Reply header: {:#?}", reply.header());

    use ac215::packet::packets::answer_firm_ver::AnswerFirmVer825Packet;
    match reply.parse::<AnswerFirmVer825Packet>() {
        Some(Ok(pkt)) => println!("Firmware: {:#?}", pkt),
        Some(Err(e)) => println!("Parse error: {}", e),
        None => println!("Unexpected packet type: 0x{:02X}", reply.header().command_id()),
    }

    let reply = server
        .request(
            Ac215Address::panel_main_controller(Ac215AddressMode::Dual),
            ManualOutputPacket::pulse(&[0, 1, 2, 3, 4, 5], 0, 4),
            ChecksumMode::Auto,
        )
        .await
        .expect("failed to send manual output");

    println!("Manual output reply header: {:#?}", reply.header());

    println!("Listening for unsolicited frames...");
    while let Some(frame) = server.recv().await {
        println!(
            "[0x{:02X}] {:?} -> {:?} ({} bytes payload)",
            frame.header().command_id(),
            frame.header().source(),
            frame.header().destination(),
            frame.payload().len(),
        );
    }
}

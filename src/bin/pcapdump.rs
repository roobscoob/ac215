use std::{collections::HashMap, env, fs, io::Cursor, process};

use ac215::{
    crypto::Cipher,
    packet::{
        Ac215Packet,
        direction::Ac215PacketDirection,
        header::{Ac215Header, ChecksumMode},
        packets::{
            answer_events_825::AnswerEvents825Packet, answer_firm_ver::AnswerFirmVer825Packet,
            send_log_message::SendLogMessagePacket, update_clock::UpdateClockPacket,
        },
    },
};
use etherparse::{SlicedPacket, TransportSlice};
use pcap_parser::traits::PcapReaderIterator;
use pcap_parser::{PcapNGReader, PcapError};

type FlowKey = ([u8; 4], u16, [u8; 4], u16);

fn flow_direction(src_port: u16, dst_port: u16) -> Ac215PacketDirection {
    if src_port < dst_port {
        Ac215PacketDirection::FromPanel
    } else {
        Ac215PacketDirection::ToPanel
    }
}

fn format_packet(header: &Ac215Header, payload: &[u8]) -> String {
    let header_line = format!(
        "[0x{:02X}] {:?} -> {:?}",
        header.command_id(),
        header.source(),
        header.destination(),
    );

    let body = match header.command_id() {
        0x6B => "    RequestFirmVer825Packet".to_string(),
        0xEB => match AnswerFirmVer825Packet::from_bytes(payload) {
            Ok(pkt) => format!("    {:#?}", pkt),
            Err(e) => format!("    AnswerFirmVer825Packet {{ error: {} }}", e),
        },
        0x62 => match UpdateClockPacket::from_bytes(payload) {
            Ok(pkt) => format!("    {:#?}", pkt),
            Err(e) => format!("    UpdateClockPacket {{ error: {} }}", e),
        },
        0x48 => "    RequestEvents825Packet".to_string(),
        0xC9 => match AnswerEvents825Packet::from_bytes(payload) {
            Ok(pkt) => format!("    {:#?}", pkt),
            Err(e) => format!("    AnswerEvents825Packet {{ error: {} }}", e),
        },
        0xD4 => match SendLogMessagePacket::from_bytes(payload) {
            Ok(pkt) => format!("    {:#?}", pkt),
            Err(e) => format!("    SendLogMessagePacket {{ error: {} }}", e),
        },
        0x50 => "    AckEvents825Packet".to_string(),
        0x80 => "    Answer80 (ACK)".to_string(),
        0x82 => "    Nack".to_string(),
        0xD0 => "    KeepAlivePacket".to_string(),
        0xD1 => "    KeepAliveAckPacket".to_string(),
        id => {
            let n = payload.len().min(32);
            format!(
                "    Unknown(0x{:02X}) {} bytes: {:02X?}{}",
                id,
                payload.len(),
                &payload[..n],
                if payload.len() > 32 { "..." } else { "" }
            )
        }
    };

    format!("{}\n{}", header_line, body)
}

struct TcpStream {
    buf: Vec<u8>,
    direction: Ac215PacketDirection,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: pcapdump <file.pcapng> [--port PORT]");
        process::exit(1);
    }

    let path = &args[1];
    let port_filter: Option<u16> = args
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse().ok());

    let file_data = fs::read(path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        process::exit(1);
    });

    let cipher = Cipher::new();
    let mut streams: HashMap<FlowKey, TcpStream> = HashMap::new();

    let mut reader = PcapNGReader::new(65536, Cursor::new(&file_data)).unwrap_or_else(|e| {
        eprintln!("Failed to parse pcap: {:?}", e);
        process::exit(1);
    });

    loop {
        match reader.next() {
            Ok((offset, block)) => {
                if let pcap_parser::PcapBlockOwned::NG(pcap_parser::Block::EnhancedPacket(epb)) =
                    block
                {
                    if let Some((key, tcp_payload)) = extract_tcp(&epb.data, port_filter) {
                        if !tcp_payload.is_empty() {
                            let direction =
                                flow_direction(key.1, key.3);
                            let stream = streams.entry(key).or_insert_with(|| TcpStream {
                                buf: Vec::new(),
                                direction,
                            });
                            stream.buf.extend_from_slice(tcp_payload);
                            drain_frames(&mut stream.buf, stream.direction, &cipher).await;
                        }
                    }
                }
                reader.consume(offset);
            }
            Err(PcapError::Eof) => break,
            Err(PcapError::Incomplete(_)) => {
                reader.refill().unwrap_or_else(|e| {
                    eprintln!("Refill error: {:?}", e);
                    process::exit(1);
                });
            }
            Err(e) => {
                eprintln!("Pcap error: {:?}", e);
                break;
            }
        }
    }
}

fn extract_tcp<'a>(
    eth_data: &'a [u8],
    port_filter: Option<u16>,
) -> Option<(FlowKey, &'a [u8])> {
    let parsed = SlicedPacket::from_ethernet(eth_data).ok()?;

    let tcp = match parsed.transport? {
        TransportSlice::Tcp(tcp) => tcp,
        _ => return None,
    };

    let (src_ip, dst_ip) = match parsed.net? {
        etherparse::NetSlice::Ipv4(ipv4) => {
            (ipv4.header().source(), ipv4.header().destination())
        }
        _ => return None,
    };

    let src_port = tcp.source_port();
    let dst_port = tcp.destination_port();

    if let Some(pf) = port_filter {
        if src_port != pf && dst_port != pf {
            return None;
        }
    }

    let payload = tcp.payload();
    let key: FlowKey = (src_ip, src_port, dst_ip, dst_port);
    Some((key, payload))
}

async fn drain_frames(buf: &mut Vec<u8>, direction: Ac215PacketDirection, cipher: &Cipher) {
    loop {
        let magic_pos = buf.windows(4).position(|w| w == [0xFF, 0xFF, 0xFF, 0x55]);
        let magic_pos = match magic_pos {
            Some(p) => p,
            None => break,
        };

        if magic_pos > 0 {
            buf.drain(..magic_pos);
        }

        if buf.len() < 5 {
            break;
        }

        let mut cursor = Cursor::new(buf.as_slice());
        match Ac215Header::parse_frame(direction, cipher.clone(), &mut cursor, ChecksumMode::Auto)
            .await
        {
            Ok((header, payload)) => {
                let consumed = cursor.position() as usize;
                println!("{}", format_packet(&header, &payload));
                buf.drain(..consumed);
            }
            Err(_) => {
                buf.drain(..4);
            }
        }
    }
}

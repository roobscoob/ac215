use crate::{
    crypto::Cipher,
    packet::{
        Ac215Packet,
        direction::Ac215PacketDirection,
        header::{Ac215Header, ChecksumMode},
        packets::{
            answer_firm_ver::AnswerFirmVer825Packet, send_log_message::SendLogMessagePacket,
            update_clock::UpdateClockPacket,
        },
    },
};

#[tokio::test]
async fn test() {
    let bytes = hex::decode("ffffff550b110de14a993b6ff5465ab08af3ab54387ce0").unwrap();
    let (header, payload) = Ac215Header::parse_frame(
        Ac215PacketDirection::FromPanel,
        Cipher::new(),
        &mut &bytes[..],
        ChecksumMode::Auto,
    )
    .await
    .unwrap();

    println!("Header: {:#?}", header);
    // println!("Payload: {:02X?}", payload);

    let packet = UpdateClockPacket::from_bytes(&payload).unwrap();

    println!("Packet: {:#?}", packet);
}

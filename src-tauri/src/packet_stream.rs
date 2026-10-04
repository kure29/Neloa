use std::{future::Future, io};

use tokio::{
    io::{split, AsyncReadExt, AsyncWriteExt, DuplexStream},
    sync::mpsc,
};

const PACKET_VERSION: u8 = 1;
const PACKET_HEADER_LEN: usize = 6;
const PACKET_FLAG_FIN: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PacketFraming {
    /// Preserve the existing packet transport behavior. Packet boundaries are
    /// not exposed to the application-facing byte stream.
    Raw,
    /// Add a small sequence header so transports such as BLE notifications can
    /// reject missing, duplicated, reordered, or malformed packets.
    Sequenced,
}

/// Bridges a packet-oriented link to the byte stream consumed by the Noise
/// session. The packet receiver and emitter are intentionally platform-neutral:
/// CoreBluetooth, Android GATT, or another native backend can provide them
/// without changing pairing or file-transfer code.
pub(crate) async fn run_packet_stream<Emit, EmitFuture>(
    stream: DuplexStream,
    mut inbound: mpsc::Receiver<Vec<u8>>,
    maximum_packet_size: usize,
    framing: PacketFraming,
    mut emit: Emit,
) -> io::Result<()>
where
    Emit: FnMut(Vec<u8>) -> EmitFuture,
    EmitFuture: Future<Output = io::Result<()>>,
{
    let payload_limit = match framing {
        PacketFraming::Raw => maximum_packet_size,
        PacketFraming::Sequenced => maximum_packet_size
            .checked_sub(PACKET_HEADER_LEN)
            .filter(|size| *size > 0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "packet size {maximum_packet_size} cannot carry the {PACKET_HEADER_LEN}-byte framing header"
                    ),
                )
            })?,
    };
    if payload_limit == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "packet size must be greater than zero",
        ));
    }

    let (mut reader, mut writer) = split(stream);
    let mut buffer = vec![0_u8; payload_limit];
    let outbound = async {
        let mut sequence = 0_u32;
        loop {
            let size = reader.read(&mut buffer).await?;
            if size == 0 {
                if framing == PacketFraming::Sequenced {
                    emit(encode_packet(sequence, PACKET_FLAG_FIN, &[])).await?;
                }
                return Ok::<(), io::Error>(());
            }

            let packet = match framing {
                PacketFraming::Raw => buffer[..size].to_vec(),
                PacketFraming::Sequenced => {
                    let packet = encode_packet(sequence, 0, &buffer[..size]);
                    sequence = next_sequence(sequence)?;
                    packet
                }
            };
            emit(packet).await?;
        }
    };

    let incoming = async {
        let mut sequence = 0_u32;
        loop {
            let Some(packet) = inbound.recv().await else {
                writer.shutdown().await?;
                return Ok::<(), io::Error>(());
            };
            match framing {
                PacketFraming::Raw => writer.write_all(&packet).await?,
                PacketFraming::Sequenced => {
                    let decoded = decode_packet(&packet, sequence)?;
                    if decoded.finished {
                        writer.shutdown().await?;
                        return Ok(());
                    }
                    writer.write_all(decoded.payload).await?;
                    sequence = next_sequence(sequence)?;
                }
            }
        }
    };

    match framing {
        // The relay protocol has its own close notification, so preserve its
        // existing behavior and close the tunnel when either side ends.
        PacketFraming::Raw => tokio::select! {
            result = outbound => result,
            result = incoming => result,
        },
        // BLE supports independent half-close packets. Keep receiving after
        // local EOF (and vice versa) so simultaneous transfers cannot deadlock.
        PacketFraming::Sequenced => {
            tokio::try_join!(outbound, incoming)?;
            Ok(())
        }
    }
}

fn next_sequence(sequence: u32) -> io::Result<u32> {
    sequence.checked_add(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "packet sequence exhausted; reconnect before continuing",
        )
    })
}

fn encode_packet(sequence: u32, flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(PACKET_HEADER_LEN + payload.len());
    packet.push(PACKET_VERSION);
    packet.push(flags);
    packet.extend_from_slice(&sequence.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

#[derive(Debug)]
struct DecodedPacket<'a> {
    payload: &'a [u8],
    finished: bool,
}

fn decode_packet(packet: &[u8], expected_sequence: u32) -> io::Result<DecodedPacket<'_>> {
    if packet.len() < PACKET_HEADER_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "packet is shorter than the framing header",
        ));
    }
    if packet[0] != PACKET_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported packet version {}", packet[0]),
        ));
    }

    let flags = packet[1];
    if flags & !PACKET_FLAG_FIN != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("packet contains unsupported flags 0x{flags:02x}"),
        ));
    }

    let sequence = u32::from_be_bytes(
        packet[2..PACKET_HEADER_LEN]
            .try_into()
            .expect("header size"),
    );
    if sequence != expected_sequence {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected packet {expected_sequence}, received {sequence}"),
        ));
    }

    let finished = flags & PACKET_FLAG_FIN != 0;
    let payload = &packet[PACKET_HEADER_LEN..];
    if finished && !payload.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "final packet must not contain a payload",
        ));
    }
    if !finished && payload.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "data packet must contain a payload",
        ));
    }

    Ok(DecodedPacket { payload, finished })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{duplex, AsyncReadExt, AsyncWriteExt},
        sync::mpsc,
        time::timeout,
    };

    use super::*;

    const TEST_STREAM_BUFFER: usize = 1024;
    const TEST_PACKET_SIZE: usize = 23;

    fn spawn_sequenced_pair() -> (
        DuplexStream,
        DuplexStream,
        tokio::task::JoinHandle<io::Result<()>>,
        tokio::task::JoinHandle<io::Result<()>>,
    ) {
        let (application_a, pump_a) = duplex(TEST_STREAM_BUFFER);
        let (application_b, pump_b) = duplex(TEST_STREAM_BUFFER);
        let (a_to_b, a_to_b_receiver) = mpsc::channel::<Vec<u8>>(1);
        let (b_to_a, b_to_a_receiver) = mpsc::channel::<Vec<u8>>(1);

        let worker_a = tokio::spawn(run_packet_stream(
            pump_a,
            b_to_a_receiver,
            TEST_PACKET_SIZE,
            PacketFraming::Sequenced,
            move |packet| {
                let sender = a_to_b.clone();
                async move {
                    sender.send(packet).await.map_err(|_| {
                        io::Error::new(io::ErrorKind::BrokenPipe, "peer packet receiver closed")
                    })
                }
            },
        ));
        let worker_b = tokio::spawn(run_packet_stream(
            pump_b,
            a_to_b_receiver,
            TEST_PACKET_SIZE,
            PacketFraming::Sequenced,
            move |packet| {
                let sender = b_to_a.clone();
                async move {
                    sender.send(packet).await.map_err(|_| {
                        io::Error::new(io::ErrorKind::BrokenPipe, "peer packet receiver closed")
                    })
                }
            },
        ));

        (application_a, application_b, worker_a, worker_b)
    }

    #[tokio::test]
    async fn sequenced_stream_fragments_and_reassembles_both_directions() {
        let (application_a, application_b, worker_a, worker_b) = spawn_sequenced_pair();
        let from_a = (0..65_537)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let from_b = (0..8_193)
            .map(|index| (index % 239) as u8)
            .collect::<Vec<_>>();

        let expected_from_a = from_a.clone();
        let expected_from_b = from_b.clone();
        let from_a_len = from_a.len();
        let from_b_len = from_b.len();
        let exchange = async move {
            let (mut read_a, mut write_a) = tokio::io::split(application_a);
            let (mut read_b, mut write_b) = tokio::io::split(application_b);
            let send_a = async move {
                write_a.write_all(&from_a).await.unwrap();
                write_a.shutdown().await.unwrap();
            };
            let receive_a = async move {
                let mut received = vec![0_u8; from_b_len];
                read_a.read_exact(&mut received).await.unwrap();
                received
            };
            let send_b = async move {
                write_b.write_all(&from_b).await.unwrap();
                write_b.shutdown().await.unwrap();
            };
            let receive_b = async move {
                let mut received = vec![0_u8; from_a_len];
                read_b.read_exact(&mut received).await.unwrap();
                received
            };
            let (_, received_by_a, _, received_by_b) =
                tokio::join!(send_a, receive_a, send_b, receive_b);
            (received_by_a, received_by_b)
        };

        let (received_by_a, received_by_b) = timeout(Duration::from_secs(5), exchange)
            .await
            .expect("packet exchange timed out");
        assert_eq!(received_by_a, expected_from_b);
        assert_eq!(received_by_b, expected_from_a);
        worker_a.await.unwrap().unwrap();
        worker_b.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn sequenced_stream_rejects_missing_or_reordered_packets() {
        let (mut application, pump) = duplex(TEST_STREAM_BUFFER);
        let (inbound, inbound_receiver) = mpsc::channel(1);
        let worker = tokio::spawn(run_packet_stream(
            pump,
            inbound_receiver,
            TEST_PACKET_SIZE,
            PacketFraming::Sequenced,
            |_| async { Ok(()) },
        ));

        inbound
            .send(encode_packet(1, 0, b"out of order"))
            .await
            .unwrap();
        let mut byte = [0_u8; 1];
        assert_eq!(application.read(&mut byte).await.unwrap(), 0);
        let error = worker.await.unwrap().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("expected packet 0, received 1"));
    }

    #[tokio::test]
    async fn bounded_packet_queue_applies_backpressure() {
        let (mut application, pump) = duplex(TEST_STREAM_BUFFER);
        let (_inbound, inbound_receiver) = mpsc::channel(1);
        let (outbound, mut outbound_receiver) = mpsc::channel::<Vec<u8>>(1);
        let worker = tokio::spawn(run_packet_stream(
            pump,
            inbound_receiver,
            TEST_PACKET_SIZE,
            PacketFraming::Sequenced,
            move |packet| {
                let sender = outbound.clone();
                async move {
                    sender.send(packet).await.map_err(|_| {
                        io::Error::new(io::ErrorKind::BrokenPipe, "packet receiver closed")
                    })
                }
            },
        ));

        let write = tokio::spawn(async move {
            application.write_all(&vec![7_u8; 8 * 1024]).await.unwrap();
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert!(!write.is_finished());

        while !write.is_finished() {
            outbound_receiver
                .recv()
                .await
                .expect("packet stream closed early");
        }
        write.await.unwrap();
        worker.abort();
    }

    #[test]
    fn sequenced_packets_validate_version_flags_and_fin_payload() {
        let mut wrong_version = encode_packet(0, 0, b"data");
        wrong_version[0] = 2;
        assert_eq!(
            decode_packet(&wrong_version, 0).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let wrong_flags = encode_packet(0, 0x80, b"data");
        assert_eq!(
            decode_packet(&wrong_flags, 0).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let fin_with_payload = encode_packet(0, PACKET_FLAG_FIN, b"data");
        assert_eq!(
            decode_packet(&fin_with_payload, 0).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}

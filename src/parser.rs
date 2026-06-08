// src/parser.rs — Bitcoin P2P message streaming parser

use std::io::{self, Read};
use std::net::TcpStream;
use crate::message::MessageHeader;
use crate::crypto::double_sha256;

#[derive(Debug)]
pub struct BitcoinMessage {
    pub header: MessageHeader,
    pub payload: Vec<u8>,
    pub command: String,
}

// Reads exactly `n` bytes from the TCP socket.
pub fn read_exact_bytes(stream: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n];
    stream.read_exact(&mut buf)?;
    Ok(buf)
}

// Reads and validates a complete message from the TCP socket.
pub fn read_message(stream: &mut TcpStream, magic: [u8; 4]) -> io::Result<BitcoinMessage> {
    loop {
        // Read 24-byte header
        let header_bytes = read_exact_bytes(stream, 24)?;
        let header = MessageHeader::deserialize(&header_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        if header.magic != magic {
            eprintln!(
                "[!] Magic mismatch! Expected {:02X?}, got {:02X?}.",
                magic, header.magic
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Magic bytes mismatch — possible wrong network or parser desync",
            ));
        }

        let payload_len = header.length as usize;

        // Prevent memory exhaustion attacks (max message size is 32MB)
        if payload_len > 32 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Payload claims to be {} bytes — exceeds 32MB max", payload_len),
            ));
        }

        let payload = if payload_len > 0 {
            read_exact_bytes(stream, payload_len)?
        } else {
            Vec::new()
        };

        // Validate checksum
        let computed_hash = double_sha256(&payload);
        let computed_checksum = &computed_hash[..4];

        if computed_checksum != header.checksum {
            eprintln!(
                "[!] Checksum mismatch for '{}': expected {:02X?}, got {:02X?} — discarding",
                header.command_string(),
                header.checksum,
                computed_checksum
            );
            continue;
        }

        let command = header.command_string();
        return Ok(BitcoinMessage { header, payload, command });
    }
}
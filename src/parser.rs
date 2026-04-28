// parser.rs — Streaming Bitcoin message parser
//
// This module is responsible for reading raw bytes from a TCP socket
// and reassembling them into complete, validated Bitcoin messages.
//
// The core challenge: TCP is a byte STREAM, not a message protocol.
// When we call `read()` on a socket, we might get:
//   - Exactly one complete message
//   - Half a message (the other half arrives later)
//   - Two messages at once (they were batched)
//   - Random fragments
//
// Our parser handles this by always reading EXACT byte counts:
//   1. Read exactly 24 bytes → parse the header
//   2. Read exactly header.length bytes → that's the payload
//   3. Validate the checksum
//   4. Return the complete message
// Then repeat for the next message.

use std::io::{self, Read};
use std::net::TcpStream;
use crate::message::MessageHeader;
use crate::crypto::double_sha256;

// A fully parsed and validated Bitcoin message, ready for processing.
//
// After `read_message()` returns this, the caller knows:
//   - The message is complete (all bytes received)
//   - The magic bytes matched (correct network)
//   - The checksum is valid (payload wasn't corrupted)
#[derive(Debug)]
pub struct BitcoinMessage {
    // The parsed 24-byte header (magic, command, length, checksum)
    pub header: MessageHeader,
    // The raw payload bytes (0..N bytes depending on message type)
    pub payload: Vec<u8>,
    // The command as a clean String — cached here to avoid re-parsing
    // the null-padded command field every time we want to match on it
    pub command: String,
}

// Read EXACTLY `n` bytes from the TCP stream into a Vec<u8>.
//
// This is the foundational I/O primitive for the entire parser.
//
// Why not just call `stream.read()`?
// Because `read()` is allowed to return fewer bytes than requested!
// It returns however many bytes are currently available — which could be 1.
// `read_exact()` loops internally until all `n` bytes have been received
// or an error occurs (connection closed, timeout, etc.).
pub fn read_exact_bytes(stream: &mut TcpStream, n: usize) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; n]; // Allocate `n` zero-filled bytes

    // `read_exact` fills the entire buffer or returns an error.
    // Possible errors:
    //   UnexpectedEof — the connection closed before we got all n bytes
    //   TimedOut      — no data arrived within the read timeout we set
    //   Other I/O errors from the OS
    stream.read_exact(&mut buf)?;

    Ok(buf)
}

// Read and return one complete, validated Bitcoin message from the stream.
//
// This function blocks until a full message arrives (or an error occurs).
// It silently discards messages with invalid checksums and tries again,
// which is more robust than failing the whole connection on one bad packet.
//
// `magic` is the expected network magic — messages with wrong magic are
// rejected immediately (they're either from the wrong network or garbage).
pub fn read_message(stream: &mut TcpStream, magic: [u8; 4]) -> io::Result<BitcoinMessage> {
    loop { // Loop allows us to retry on checksum failures without returning an error
        // ── Step 1: Read the fixed-size message header (always 24 bytes) ──
        // We always know exactly how many bytes the header is, so this is simple.
        let header_bytes = read_exact_bytes(stream, 24)?;

        // ── Step 2: Parse the header fields from the raw bytes ────────────
        // This just reads the bytes at the right offsets — no I/O involved.
        let header = MessageHeader::deserialize(&header_bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // ── Step 3: Validate the network magic bytes ──────────────────────
        // If the magic doesn't match, we're either:
        //   a) Connected to a node on a different network (mainnet vs testnet)
        //   b) The parser has desynchronized and is reading from the middle
        //      of a previous message
        // Either way, this is a fatal error for this connection.
        if header.magic != magic {
            eprintln!(
                "[!] Magic mismatch! Expected {:02X?}, got {:02X?}. Wrong network?",
                magic, header.magic
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Magic bytes mismatch — possible wrong network or parser desync",
            ));
        }

        // ── Step 4: Read the payload ───────────────────────────────────────
        // The header tells us exactly how many bytes to read.
        let payload_len = header.length as usize;

        // Safety check: Bitcoin's protocol max message size is 32 megabytes.
        // If a message claims to be larger, it's either corrupt or malicious.
        // Without this check, an attacker could send a fake header claiming
        // a 4GB payload and exhaust our memory.
        if payload_len > 32 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Payload claims to be {} bytes — exceeds 32MB max", payload_len),
            ));
        }

        // Read the payload bytes, or use an empty Vec for zero-length payloads
        // (messages like `verack` and `getaddr` have no payload body)
        let payload = if payload_len > 0 {
            read_exact_bytes(stream, payload_len)?
        } else {
            Vec::new()
        };

        // ── Step 5: Validate the checksum ─────────────────────────────────
        // Compute what the checksum SHOULD be, then compare to what the
        // sender put in the header. A mismatch means data corruption in transit.
        //
        // We `continue` on mismatch (try reading the next message) rather than
        // returning an error — this is more resilient on flaky connections.
        let computed_hash = double_sha256(&payload);
        let computed_checksum = &computed_hash[..4]; // First 4 bytes of the 32-byte hash

        if computed_checksum != header.checksum {
            eprintln!(
                "[!] Checksum mismatch for '{}': expected {:02X?}, got {:02X?} — discarding",
                header.command_string(),
                header.checksum,
                computed_checksum
            );
            // Don't return an error — just skip this corrupted message and
            // try to read the next one from the stream
            continue;
        }

        // ── Step 6: Build and return the completed message ────────────────
        // Everything checked out: correct magic, correct checksum, complete payload.
        let command = header.command_string();

        return Ok(BitcoinMessage { header, payload, command });
    }
}
// message.rs — Bitcoin message header structure and framing
//
// Every single message sent on the Bitcoin P2P network — whether it's
// a version handshake, a ping, a full block, or a transaction — is wrapped
// in the same 24-byte header. This file defines that header.
//
// Bitcoin message layout:
//
//   ┌───────────────────────────────────────────────────────────┐
//   │                    HEADER (24 bytes)                      │
//   ├──────────────┬────────────────┬────────────┬─────────────┤
//   │ magic        │ command        │ length     │ checksum    │
//   │ (4 bytes)    │ (12 bytes)     │ (4 bytes)  │ (4 bytes)   │
//   ├──────────────┴────────────────┴────────────┴─────────────┤
//   │                    PAYLOAD (0..N bytes)                   │
//   └───────────────────────────────────────────────────────────┘

use crate::crypto::double_sha256;

// ── Network Magic Constants ───────────────────────────────────────────────────
//
// These 4-byte sequences identify which Bitcoin network a message belongs to.
// A node on mainnet will immediately reject messages with testnet magic bytes
// and vice versa. They also help re-synchronize a parser that has lost its
// place in the byte stream — you can scan forward looking for the magic bytes.

pub const MAGIC_MAINNET: [u8; 4] = [0xF9, 0xBE, 0xB4, 0xD9]; // Production network
pub const MAGIC_TESTNET: [u8; 4] = [0x0B, 0x11, 0x09, 0x07]; // Testnet3 (our target)
pub const MAGIC_REGTEST: [u8; 4] = [0xFA, 0xBF, 0xB5, 0xDA]; // Local regression test

// ── Message Header ────────────────────────────────────────────────────────────

// Represents the 24-byte fixed-size header that precedes every Bitcoin message.
//
// `#[derive(Debug, Clone)]` gives us:
//   - Debug: lets us print the struct with {:?} for logging/debugging
//   - Clone: lets us duplicate the struct cheaply (all fields are Copy types)
#[derive(Debug, Clone)]
pub struct MessageHeader {
    // 4-byte network identifier — must match the expected network or discard
    pub magic: [u8; 4],
    // ASCII command name, right-padded with null bytes (\x00) to fill 12 bytes
    // Example: "version" → [76,65,72,73,69,6F,6E,00,00,00,00,00]
    pub command: [u8; 12],
    // Number of bytes in the payload that follows this header (little-endian u32)
    pub length: u32,
    // First 4 bytes of double-SHA256(payload) — used to detect corruption
    pub checksum: [u8; 4],
}

impl MessageHeader {
    // Construct a new MessageHeader by computing all fields automatically.
    //
    // `command`: human-readable command name like "version", "ping", "verack"
    // `payload`: the raw bytes of the message body (can be empty)
    // `magic`:   which network magic to use (use the MAGIC_* constants above)
    pub fn new(command: &str, payload: &[u8], magic: [u8; 4]) -> Self {

        // Build the fixed-width 12-byte command field.
        // Start with 12 zero bytes (the padding), then copy the command
        // string bytes into the front. Any bytes beyond the string stay zero.
        let mut cmd_bytes = [0u8; 12];
        let bytes = command.as_bytes(); // Rust strings are UTF-8; command is pure ASCII
        let len = bytes.len().min(12);  // Clamp to 12 — we can't exceed the field width
        cmd_bytes[..len].copy_from_slice(&bytes[..len]);

        // Compute the checksum: double-SHA256 the payload, take the first 4 bytes.
        // For an empty payload (like `verack`), this is always 5D F6 E0 E2.
        let hash = double_sha256(payload);
        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&hash[..4]); // Slice the first 4 bytes of the 32-byte hash

        MessageHeader {
            magic,
            command: cmd_bytes,
            length: payload.len() as u32, // Cast usize → u32 (max payload is 32MB, fits in u32)
            checksum,
        }
    }

    // Serialize this header into its 24-byte binary wire format.
    //
    // The byte layout is exactly:
    //   [0..4]   magic       (4 bytes, as-is)
    //   [4..16]  command     (12 bytes, null-padded)
    //   [16..20] length      (4 bytes, little-endian u32)
    //   [20..24] checksum    (4 bytes, as-is)
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24); // Pre-allocate exactly 24 bytes

        buf.extend_from_slice(&self.magic);              // 4 bytes
        buf.extend_from_slice(&self.command);            // 12 bytes
        buf.extend_from_slice(&self.length.to_le_bytes()); // 4 bytes, little-endian
        buf.extend_from_slice(&self.checksum);           // 4 bytes
        // Total: 24 bytes

        buf
    }

    // Deserialize a 24-byte slice into a MessageHeader.
    //
    // This is the inverse of `serialize()`. We read each field from the
    // correct byte offset in the slice.
    // Returns Err if the slice is shorter than 24 bytes.
    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        if data.len() < 24 {
            return Err(format!(
                "Header too short: got {} bytes, need 24", data.len()
            ));
        }

        // Read magic: bytes 0..4
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[0..4]);

        // Read command: bytes 4..16
        let mut command = [0u8; 12];
        command.copy_from_slice(&data[4..16]);

        // Read length: bytes 16..20, little-endian u32
        // `try_into()` converts &[u8] into [u8; 4] — safe here because
        // we've verified data.len() >= 24, so these slices are valid.
        let length = u32::from_le_bytes(data[16..20].try_into().unwrap());

        // Read checksum: bytes 20..24
        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&data[20..24]);

        Ok(MessageHeader { magic, command, length, checksum })
    }

    // Extract the command as a clean human-readable String.
    //
    // The raw command field is always 12 bytes, null-padded on the right.
    // This method strips the null bytes and returns just the meaningful part.
    // Example: [76,65,72,73,69,6F,6E,00,00,00,00,00] → "version"
    pub fn command_string(&self) -> String {
        // Find the index of the first null byte; everything before it is the command.
        // `position()` returns None if there are no null bytes — then we use all 12.
        let end = self.command.iter().position(|&b| b == 0).unwrap_or(12);
        // `from_utf8_lossy` handles any unexpected non-UTF8 bytes gracefully
        String::from_utf8_lossy(&self.command[..end]).to_string()
    }
}

// Assemble a complete Bitcoin message (header + payload) into a byte vector.
//
// This is the single function you call to turn a command + payload into
// bytes that are ready to write to a TCP socket.
//
// Internally it:
//   1. Creates a MessageHeader (which computes the checksum automatically)
//   2. Serializes the header to 24 bytes
//   3. Appends the payload bytes
//   4. Returns the combined buffer
pub fn build_message(command: &str, payload: &[u8], magic: [u8; 4]) -> Vec<u8> {
    let header = MessageHeader::new(command, payload, magic);

    // Pre-allocate space for the full message to avoid reallocation
    let mut message = header.serialize();         // 24 bytes header
    message.extend_from_slice(payload);           // 0..N bytes payload
    message
}
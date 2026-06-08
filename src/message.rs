
// Layout:
//   ┌───────────────────────────────────────────────────────────┐
//   │                    HEADER (24 bytes)                      │
//   ├──────────────┬────────────────┬────────────┬─────────────┤
//   │ magic        │ command        │ length     │ checksum    │
//   │ (4 bytes)    │ (12 bytes)     │ (4 bytes)  │ (4 bytes)   │
//   ├──────────────┴────────────────┴────────────┴─────────────┤
//   │                    PAYLOAD (0..N bytes)                   │
//   └───────────────────────────────────────────────────────────┘

use crate::crypto::double_sha256;

// Network magic constants for testnet3
pub const MAGIC_TESTNET: [u8; 4] = [0x0B, 0x11, 0x09, 0x07];

#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub magic: [u8; 4],
    pub command: [u8; 12],
    pub length: u32,
    pub checksum: [u8; 4],
}

impl MessageHeader {
    pub fn new(command: &str, payload: &[u8], magic: [u8; 4]) -> Self {
        let mut cmd_bytes = [0u8; 12];
        let bytes = command.as_bytes();
        let len = bytes.len().min(12);
        cmd_bytes[..len].copy_from_slice(&bytes[..len]);

        let hash = double_sha256(payload);
        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&hash[..4]);

        MessageHeader {
            magic,
            command: cmd_bytes,
            length: payload.len() as u32,
            checksum,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24);
        buf.extend_from_slice(&self.magic);
        buf.extend_from_slice(&self.command);
        buf.extend_from_slice(&self.length.to_le_bytes());
        buf.extend_from_slice(&self.checksum);
        buf
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, String> {
        if data.len() < 24 {
            return Err(format!(
                "Header too short: got {} bytes, need 24", data.len()
            ));
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[0..4]);

        let mut command = [0u8; 12];
        command.copy_from_slice(&data[4..16]);

        let length = u32::from_le_bytes(data[16..20].try_into().unwrap());

        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&data[20..24]);

        Ok(MessageHeader { magic, command, length, checksum })
    }

    pub fn command_string(&self) -> String {
        let end = self.command.iter().position(|&b| b == 0).unwrap_or(12);
        String::from_utf8_lossy(&self.command[..end]).to_string()
    }
}

pub fn build_message(command: &str, payload: &[u8], magic: [u8; 4]) -> Vec<u8> {
    let header = MessageHeader::new(command, payload, magic);
    let mut message = header.serialize();
    message.extend_from_slice(payload);
    message
}
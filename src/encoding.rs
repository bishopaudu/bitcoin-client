// Encode a u64 value as a Bitcoin varint byte sequence.
pub fn encode_varint(n: u64) -> Vec<u8> {
    if n < 0xFD {
        // Fits in a single byte — no prefix needed, just the value itself
        vec![n as u8]
    } else if n <= 0xFFFF {
        // Needs 2 bytes of data: use the 0xFD prefix to signal this
        let mut v = vec![0xFD];
        v.extend_from_slice(&(n as u16).to_le_bytes()); // 2-byte little-endian
        v
    } else if n <= 0xFFFF_FFFF {
        // Needs 4 bytes of data: use the 0xFE prefix
        let mut v = vec![0xFE];
        v.extend_from_slice(&(n as u32).to_le_bytes()); // 4-byte little-endian
        v
    } else {
        // Needs 8 bytes of data: use the 0xFF prefix
        let mut v = vec![0xFF];
        v.extend_from_slice(&n.to_le_bytes()); // 8-byte little-endian
        v
    }
}

// Decode a varint from a byte slice starting at `offset`.
pub fn decode_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    // Guard: make sure there's at least one byte to read
    if offset >= data.len() {
        return Err("Buffer too short for varint".to_string());
    }

    match data[offset] {
        // Single-byte case: the byte itself is the value
        n if n < 0xFD => Ok((n as u64, 1)),

        // 0xFD prefix: next 2 bytes are a little-endian u16
        0xFD => {
            if offset + 3 > data.len() {
                return Err("Buffer too short for 2-byte varint".to_string());
            }
            let val = u16::from_le_bytes([data[offset + 1], data[offset + 2]]);
            Ok((val as u64, 3)) // 1 prefix byte + 2 data bytes = 3 consumed
        }

        // 0xFE prefix: next 4 bytes are a little-endian u32
        0xFE => {
            if offset + 5 > data.len() {
                return Err("Buffer too short for 4-byte varint".to_string());
            }
            let val = u32::from_le_bytes([
                data[offset + 1], data[offset + 2],
                data[offset + 3], data[offset + 4],
            ]);
            Ok((val as u64, 5)) // 1 prefix byte + 4 data bytes = 5 consumed
        }

        // 0xFF prefix: next 8 bytes are a little-endian u64
        _ => {
            if offset + 9 > data.len() {
                return Err("Buffer too short for 8-byte varint".to_string());
            }
            let val = u64::from_le_bytes([
                data[offset + 1], data[offset + 2],
                data[offset + 3], data[offset + 4],
                data[offset + 5], data[offset + 6],
                data[offset + 7], data[offset + 8],
            ]);
            Ok((val as u64, 9)) // 1 prefix byte + 8 data bytes = 9 consumed
        }
    }
}

// ── Variable-Length String (varstr) ──────────────────────────────────────────
pub fn encode_varstr(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes(); // Convert the string to a raw byte slice
    // Start with the length prefix (as a varint), then append the actual bytes
    let mut v = encode_varint(bytes.len() as u64);
    v.extend_from_slice(bytes);
    v
}

pub fn encode_net_addr(services: u64, ip: [u8; 4], port: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(26); // Pre-allocate exactly 26 bytes

    // Services bitmask (8 bytes, little-endian)
    v.extend_from_slice(&services.to_le_bytes());

    // IPv4-mapped IPv6 prefix: 10 zero bytes followed by 0xFF 0xFF
    // This is RFC 4291 §2.5.5.2 format for representing IPv4 inside IPv6
    v.extend_from_slice(&[0u8; 10]); // Ten zero bytes
    v.extend_from_slice(&[0xFF, 0xFF]); // IPv4-mapped marker

    // The actual 4-byte IPv4 address
    v.extend_from_slice(&ip);

    // Port in big-endian (the one exception to Bitcoin's little-endian convention)
    v.extend_from_slice(&port.to_be_bytes());

    v // Total: 8 + 10 + 2 + 4 + 2 = 26 bytes
}
// src/encoding.rs — Bitcoin protocol encoding/decoding utilities

// Encode a u64 value as a Bitcoin varint.
pub fn encode_varint(n: u64) -> Vec<u8> {
    if n < 0xFD {
        vec![n as u8]
    } else if n <= 0xFFFF {
        let mut v = vec![0xFD];
        v.extend_from_slice(&(n as u16).to_le_bytes());
        v
    } else if n <= 0xFFFF_FFFF {
        let mut v = vec![0xFE];
        v.extend_from_slice(&(n as u32).to_le_bytes());
        v
    } else {
        let mut v = vec![0xFF];
        v.extend_from_slice(&n.to_le_bytes());
        v
    }
}

// Decode a varint from a byte slice starting at `offset`.
pub fn decode_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    if offset >= data.len() {
        return Err("Buffer too short for varint".to_string());
    }

    match data[offset] {
        n if n < 0xFD => Ok((n as u64, 1)),
        0xFD => {
            if offset + 3 > data.len() {
                return Err("Buffer too short for 2-byte varint".to_string());
            }
            let val = u16::from_le_bytes([data[offset + 1], data[offset + 2]]);
            Ok((val as u64, 3))
        }
        0xFE => {
            if offset + 5 > data.len() {
                return Err("Buffer too short for 4-byte varint".to_string());
            }
            let val = u32::from_le_bytes([
                data[offset + 1], data[offset + 2],
                data[offset + 3], data[offset + 4],
            ]);
            Ok((val as u64, 5))
        }
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
            Ok((val as u64, 9))
        }
    }
}

// Encode a string as a variable-length string (varstr).
pub fn encode_varstr(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut v = encode_varint(bytes.len() as u64);
    v.extend_from_slice(bytes);
    v
}

// Encode services, IP, and port into the 26-byte Bitcoin net_addr format.
pub fn encode_net_addr(services: u64, ip: [u8; 4], port: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(26);

    v.extend_from_slice(&services.to_le_bytes());

    // IPv4-mapped IPv6 address prefix (RFC 4291 §2.5.5.2)
    v.extend_from_slice(&[0u8; 10]);
    v.extend_from_slice(&[0xFF, 0xFF]);
    v.extend_from_slice(&ip);

    // Port in big-endian (network byte order)
    v.extend_from_slice(&port.to_be_bytes());

    v
}
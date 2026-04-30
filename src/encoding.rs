// encoding.rs — Bitcoin wire protocol encoding and decoding helpers
//
// Bitcoin's binary protocol uses several custom encoding types
// that appear repeatedly across different message types:
//
//   varint   — a variable-length integer (1, 3, 5, or 9 bytes)
//   varstr   — a length-prefixed UTF-8 string (varint + bytes)
//   net_addr — a 26-byte network address struct (services + IP + port)
//
// These are stateless helper functions with no I/O — they only
// transform data between Rust types and raw byte vectors.

// Variable-Length Integer (varint)
//
// Bitcoin uses varints to compactly represent lengths and counts.
// Instead of always using 4 or 8 bytes, small numbers use fewer bytes:
//
//   Value range          Encoding
//   ──────────────────── ──────────────────────────────────────────────
//   0x00  ..= 0xFC       1 byte:  [value]
//   0xFD  ..= 0xFFFF     3 bytes: [0xFD, value_lo, value_hi]
//   0x10000..=0xFFFFFFFF 5 bytes: [0xFE, b0, b1, b2, b3]
//   larger               9 bytes: [0xFF, b0..b7]
//
// This saves bandwidth: most messages have fewer than 253 items,
// so they need only 1 byte for the count instead of 4 or 8.

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
//
// Returns a tuple of (decoded_value, bytes_consumed).
// `bytes_consumed` tells the caller how many bytes to advance their offset.
//
// Example:
//   data = [0xFD, 0xE8, 0x03, ...]  → value = 1000, consumed = 3
//   data = [0x07, ...]              → value = 7,    consumed = 1
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
//
// A varstr is simply: varint(byte_length) followed by the raw UTF-8 bytes.
// Used in the `version` message for the user_agent field.
//
// Example: "hi" → [0x02, 0x68, 0x69]
//                   len=2  'h'   'i'

// Encode a Rust &str as a Bitcoin varstr byte sequence.
pub fn encode_varstr(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes(); // Convert the string to a raw byte slice
    // Start with the length prefix (as a varint), then append the actual bytes
    let mut v = encode_varint(bytes.len() as u64);
    v.extend_from_slice(bytes);
    v
}

//Network Address (net_addr) 
//
// A 26-byte structure used in `version` and `addr` messages to describe
// a peer's network address. Layout:
//
//   Bytes  Field     Description
//   ─────  ────────  ──────────────────────────────────────────────────
//   0..8   services  64-bit bitmask of supported features (little-endian)
//   8..24  ip        16-byte IPv6 address (or IPv4-mapped-IPv6 for IPv4 peers)
//   24..26 port      16-bit port number in BIG-ENDIAN (network byte order)
//
// The big-endian port is a historical quirk — the format was borrowed from
// IPv6 socket structures, which use network byte order (big-endian) for ports.
// Everything else in Bitcoin is little-endian, but this field is the exception.
//
// IPv4 addresses are encoded as "IPv4-mapped IPv6":
//   ::ffff:a.b.c.d  →  [0,0,0,0,0,0,0,0,0,0, 0xFF,0xFF, a,b,c,d]
//   10 zero bytes + 2 bytes 0xFF + 4 bytes IPv4

// Encode a 26-byte network address structure.
//
// `services`: the capability bitmask for this address
// `ip`:       4-byte IPv4 address octets
// `port`:     port number (will be encoded big-endian)
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
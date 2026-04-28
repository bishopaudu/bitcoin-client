// version.rs — Bitcoin `version` message construction and decoding
//
// The `version` message is the FIRST message sent after a TCP connection
// is established. It is mandatory — no other messages will be accepted
// by the remote node until the version handshake completes.
//
// Handshake sequence:
//   You ──► version ──► Peer      "Here is who I am"
//   You ◄── version ◄── Peer      "Here is who I am"
//   You ◄── verack  ◄── Peer      "I acknowledge your version"
//   You ──► verack  ──► Peer      "I acknowledge your version"
//   [handshake complete — normal messaging begins]
//
// The version payload contains:
//   - Protocol version number
//   - Feature flags (services bitmask)
//   - Current timestamp
//   - Receiver's network address
//   - Sender's network address
//   - Random nonce (for loop detection)
//   - User agent string (software identifier)
//   - Best block height
//   - Relay flag (do you want unconfirmed transactions?)

use std::time::{SystemTime, UNIX_EPOCH};
use crate::encoding::{encode_varstr, encode_net_addr, decode_varint};

// Generate a pseudo-random 64-bit nonce for use in the version message.
//
// The nonce serves one purpose: loop detection. If we receive back the same
// nonce we sent, we know we have accidentally connected to ourselves (our own
// node, perhaps via NAT reflection) and should immediately disconnect.
//
// In production code you would use a cryptographically secure RNG (e.g.
// the `rand` crate with OsRng). Here we derive entropy from the nanosecond
// timestamp and some bit manipulation to keep the code dependency-free.
pub fn generate_nonce() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()      // Panics only if system clock is set before Jan 1 1970
        .as_nanos() as u64;
    // XOR with a rotated copy of itself to spread entropy across all 64 bits.
    // Without this, low nanosecond values would leave the high bytes near-zero.
    t ^ t.rotate_left(32) ^ 0xDEADBEEFCAFEBABE
}

// Build the raw byte payload for a Bitcoin `version` message.
//
// `receiver_ip`:   the 4-byte IPv4 address of the node we're connecting to
// `receiver_port`: the port of the node we're connecting to (usually 18333 testnet)
//
// Returns a Vec<u8> ready to be used as the payload in build_message().
pub fn build_version_payload(receiver_ip: [u8; 4], receiver_port: u16) -> Vec<u8> {
    let mut payload = Vec::new();

    // ── Field 1: Protocol Version (int32, little-endian, 4 bytes) ────────
    // 70015 is the current Bitcoin protocol version used by Bitcoin Core.
    // It signals support for: SegWit, compact blocks, fee filters, and more.
    // If we sent an older version number (e.g. 60001), the peer might not
    // send us newer message types that we'd miss.
    let version: i32 = 70015;
    payload.extend_from_slice(&version.to_le_bytes());

    // ── Field 2: Services (uint64, little-endian, 8 bytes) ────────────────
    // A bitmask describing what services we offer to the network.
    // Common flags:
    //   0x01 = NODE_NETWORK       — we are a full node and can serve blocks
    //   0x04 = NODE_BLOOM         — we support bloom filter requests (BIP 37)
    //   0x08 = NODE_WITNESS       — we support SegWit (BIP 141)
    //   0x0400 = NODE_NETWORK_LIMITED — pruned node, limited block history
    //
    // We advertise 0x00 because we are a pure observer — we don't store
    // blocks and can't serve data to anyone.
    let services: u64 = 0x00;
    payload.extend_from_slice(&services.to_le_bytes());

    // ── Field 3: Timestamp (int64, little-endian, 8 bytes) ────────────────
    // The current Unix time in seconds (seconds since Jan 1, 1970 UTC).
    // Bitcoin nodes use this field to detect clock skew across the network.
    // If your timestamp differs from the network median by more than 70 minutes,
    // Bitcoin Core will display a warning and may affect time-sensitive logic.
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock is before Unix epoch — check your clock")
        .as_secs() as i64; // Cast to i64 because the field is signed on the wire
    payload.extend_from_slice(&timestamp.to_le_bytes());

    // ── Field 4: addr_recv (26 bytes) ─────────────────────────────────────
    // The network address of the node RECEIVING this message (our peer).
    // We claim their services are NODE_NETWORK (1) — a reasonable assumption
    // since they're a live testnet node. The peer ignores this field in practice.
    payload.extend_from_slice(&encode_net_addr(1, receiver_ip, receiver_port));

    // ── Field 5: addr_from (26 bytes) ─────────────────────────────────────
    // Our own network address — who WE are. This field is largely ignored
    // by modern Bitcoin nodes for two reasons:
    //   1. Most nodes are behind NAT and don't know their external IP
    //   2. Bitcoin Core deprecated relying on this field years ago
    // We send all zeros: services=0, ip=0.0.0.0, port=0.
    payload.extend_from_slice(&encode_net_addr(0, [0, 0, 0, 0], 0));

    // ── Field 6: Nonce (uint64, 8 bytes) ──────────────────────────────────
    // A random 8-byte value unique to this connection attempt.
    // If the peer echoes back a nonce that matches one we sent on another
    // connection, we know we've connected to ourselves and must disconnect.
    // This prevents nodes from wasting resources talking to themselves.
    let nonce: u64 = generate_nonce();
    payload.extend_from_slice(&nonce.to_le_bytes());

    // ── Field 7: User Agent (varstr) ──────────────────────────────────────
    // A human-readable string identifying our software, following the
    // convention: /SoftwareName:VersionNumber/
    // Examples from real nodes:
    //   "/Satoshi:25.0.0/"         ← Bitcoin Core
    //   "/btcd:0.23.3/"            ← btcd (Go implementation)
    //   "/rust-bitcoin-client:0.1.0/"  ← ours
    // This is purely informational — nodes don't validate or act on it.
    let user_agent = "/rust-bitcoin-client:0.1.0/";
    payload.extend_from_slice(&encode_varstr(user_agent));

    // ── Field 8: Start Height (int32, little-endian, 4 bytes) ────────────
    // The block height of the best block we currently know about.
    // This tells the peer how much of the blockchain we have synced.
    //   0 = we only know the genesis block (we haven't synced at all)
    //   800000 = we're fully synced to the current tip (as of ~2024)
    // We send 0 because this is a fresh observer node with no blockchain data.
    let start_height: i32 = 0;
    payload.extend_from_slice(&start_height.to_le_bytes());

    // ── Field 9: Relay (bool, 1 byte) ─────────────────────────────────────
    // Added in protocol version 70001 (BIP 37).
    // 0x01 = please relay unconfirmed transactions to me
    // 0x00 = I only want blocks, skip transaction relay (SPV clients do this
    //        to reduce bandwidth usage — they only care about confirmed txs)
    // We set 0x01 so we can observe transaction propagation on the network.
    payload.push(0x01);

    payload // Return the complete serialized version payload
}

// Decode and describe a received `version` message payload for logging.
//
// When the remote peer sends us their version message, we parse the key
// fields and return a human-readable summary string. This is used purely
// for display/debugging — we don't need to act on most of these fields.
//
// Returns Err if the payload is too short or malformed.
pub fn decode_version_payload(payload: &[u8]) -> Result<String, String> {
    // Minimum valid version payload:
    //   4 (version) + 8 (services) + 8 (timestamp) + 26 (addr_recv) = 46 bytes
    if payload.len() < 46 {
        return Err(format!("Version payload too short: {} bytes", payload.len()));
    }

    // Read version: bytes 0..4 (int32 little-endian)
    let version = i32::from_le_bytes(payload[0..4].try_into().unwrap());

    // Read services: bytes 4..12 (uint64 little-endian)
    let services = u64::from_le_bytes(payload[4..12].try_into().unwrap());

    // Read timestamp: bytes 12..20 (int64 little-endian)
    let timestamp = i64::from_le_bytes(payload[12..20].try_into().unwrap());

    // Skip addr_recv (26 bytes) and addr_from (26 bytes) = 52 bytes total
    // We don't use these fields in our observer client
    let mut offset = 20 + 52;

    // Read nonce: 8 bytes uint64
    if offset + 8 > payload.len() {
        return Err("Payload truncated at nonce".to_string());
    }
    let nonce = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
    offset += 8;

    // Read user_agent: varint(length) + UTF-8 bytes
    // decode_varint returns (value, bytes_consumed) so we advance offset by both
    let (ua_len, varint_size) = decode_varint(payload, offset)?;
    offset += varint_size; // Move past the varint prefix
    let ua_end = offset + ua_len as usize;
    if ua_end > payload.len() {
        return Err("Payload truncated at user_agent".to_string());
    }
    let user_agent = String::from_utf8_lossy(&payload[offset..ua_end]).to_string();
    offset = ua_end; // Advance past the user agent string bytes

    // Read start_height: int32 little-endian
    if offset + 4 > payload.len() {
        return Err("Payload truncated at start_height".to_string());
    }
    let start_height = i32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());

    // Format a clean summary string for logging
    Ok(format!(
        "version={} services=0x{:016X} timestamp={} nonce={:016X} user_agent='{}' height={}",
        version, services, timestamp, nonce, user_agent, start_height
    ))
}
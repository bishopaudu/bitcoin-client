// src/version.rs — Bitcoin `version` message construction and decoding

use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;
use crate::encoding::{encode_varstr, encode_net_addr, decode_varint};

// Generates a random 64-bit nonce for the version handshake to detect self-connections.
pub fn generate_nonce() -> u64 {
    let mut random = rand::rng();
    random.next_u64()
}

// Builds the payload for a Bitcoin `version` message.
pub fn build_version_payload(receiver_ip: [u8; 4], receiver_port: u16) -> Vec<u8> {
    let mut payload = Vec::new();

    // 1. Protocol Version (70015 signals SegWit/FeeFilter support)
    let version: i32 = 70015;
    payload.extend_from_slice(&version.to_le_bytes());

    // 2. Services (0x00 because we are an observer node and do not serve blocks)
    let services: u64 = 0x00;
    payload.extend_from_slice(&services.to_le_bytes());

    // 3. Timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System clock is before Unix epoch")
        .as_secs() as i64;
    payload.extend_from_slice(&timestamp.to_le_bytes());

    // 4. addr_recv (Receiver's network address)
    payload.extend_from_slice(&encode_net_addr(1, receiver_ip, receiver_port));

    // 5. addr_from (Sender's network address — unused, sent as all zeros)
    payload.extend_from_slice(&encode_net_addr(0, [0, 0, 0, 0], 0));

    // 6. Nonce
    let nonce: u64 = generate_nonce();
    payload.extend_from_slice(&nonce.to_le_bytes());

    // 7. User Agent
    let user_agent = "/rust-bitcoin-client:0.1.0/";
    payload.extend_from_slice(&encode_varstr(user_agent));

    // 8. Start Height (0 because we don't have block data)
    let start_height: i32 = 0;
    payload.extend_from_slice(&start_height.to_le_bytes());

    // 9. Relay flag (0x01 indicates we want unconfirmed transactions relayed)
    payload.push(0x01);

    payload
}

// Decodes a received `version` message payload into a human-readable summary string.
pub fn decode_version_payload(payload: &[u8]) -> Result<String, String> {
    if payload.len() < 46 {
        return Err(format!("Version payload too short: {} bytes", payload.len()));
    }

    let version = i32::from_le_bytes(payload[0..4].try_into().unwrap());
    let services = u64::from_le_bytes(payload[4..12].try_into().unwrap());
    let timestamp = i64::from_le_bytes(payload[12..20].try_into().unwrap());

    // Skip addr_recv (26 bytes) and addr_from (26 bytes)
    let mut offset = 20 + 52;

    if offset + 8 > payload.len() {
        return Err("Payload truncated at nonce".to_string());
    }
    let nonce = u64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap());
    offset += 8;

    let (ua_len, varint_size) = decode_varint(payload, offset)?;
    offset += varint_size;
    let ua_end = offset + ua_len as usize;
    if ua_end > payload.len() {
        return Err("Payload truncated at user_agent".to_string());
    }
    let user_agent = String::from_utf8_lossy(&payload[offset..ua_end]).to_string();
    offset = ua_end;

    if offset + 4 > payload.len() {
        return Err("Payload truncated at start_height".to_string());
    }
    let start_height = i32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap());

    Ok(format!(
        "version={} services=0x{:016X} timestamp={} nonce={:016X} user_agent='{}' height={}",
        version, services, timestamp, nonce, user_agent, start_height
    ))
}
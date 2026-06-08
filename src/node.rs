// src/node.rs — Background Bitcoin P2P thread and event handling

use std::sync::{Arc, Mutex};
use std::net::TcpStream;
use tauri::{AppHandle, Manager};
use serde::Serialize;
use crate::network::find_testnet_peers;
use crate::peer::{connect_to_peer, send_message};
use crate::version::{build_version_payload, generate_nonce};
use crate::parser::read_message;
use crate::message::MAGIC_TESTNET;
use crate::encoding::decode_varint;
use crate::crypto::{double_sha256, hex_encode};
use crate::version::decode_version_payload;
use crate::transaction::parse_transaction;

// Event structs emitted to JavaScript
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    pub status: String,
    pub message: String,
    pub peer_address: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageEvent {
    pub command: String,
    pub summary: String,
    pub timestamp: u64,
    pub message_number: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfoEvent {
    pub protocol_version: i32,
    pub user_agent: String,
    pub start_height: i32,
    pub services: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransactionEvent {
    pub txid: String,
    pub fetched_from: String,
    pub version: i32,
    pub is_segwit: bool,
    pub input_count: usize,
    pub output_count: usize,
    pub total_output_sats: u64,
    pub outputs: Vec<String>,
    pub inputs: Vec<String>,
    pub locktime: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MempoolEvent {
    pub total_count: u64,
    pub txids: Vec<String>,
}

// Live Inventory Activity (inv)
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InvItem {
    pub item_type: String, // "TX" or "BLOCK"
    pub hash: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InvAnnouncement {
    pub items: Vec<InvItem>,
    pub timestamp: u64,
}

// Shared application state
pub struct NodeState {
    pub stream: Option<TcpStream>,
    pub is_connected: bool,
    pub should_run: bool,
    pub mempool_requested: bool,
    pub mempool_done: bool,
}

impl NodeState {
    pub fn new() -> Self {
        NodeState {
            stream: None,
            is_connected: false,
            should_run: false,
            mempool_requested: false,
            mempool_done: false,
        }
    }
}

pub type SharedState = Arc<Mutex<NodeState>>;

// Background worker thread for Bitcoin P2P communication
pub fn run_bitcoin_node(app_handle: AppHandle, state: SharedState) {
    let magic = MAGIC_TESTNET;

    let emit_connection = |status: &str, message: &str, peer: &str| {
        let _ = app_handle.emit_all("connection-status", ConnectionEvent {
            status: status.to_string(),
            message: message.to_string(),
            peer_address: peer.to_string(),
        });
    };

    emit_connection("connecting", "Resolving DNS seeds...", "");

    let mut candidates = find_testnet_peers();
    if candidates.is_empty() {
        candidates = vec![
            "185.210.125.33:18333".to_string(),
            "77.163.221.171:18333".to_string(),
        ];
    }

    emit_connection("connecting", &format!("Trying {} candidates...", candidates.len()), "");

    let (stream, peer_addr) = {
        let mut found = None;
        for addr in &candidates {
            match connect_to_peer(addr) {
                Ok(s) => { found = Some((s, addr.clone())); break; }
                Err(_) => continue,
            }
        }
        match found {
            Some(x) => x,
            None => {
                emit_connection("error", "Could not connect to any peer", "");
                // Mark should_run = false so callers know we stopped
                if let Ok(mut s) = state.lock() {
                    s.should_run = false;
                }
                return; // Exit the thread
            }
        }
    };

    emit_connection("connected", "TCP connection established", &peer_addr);

    let socket_addr: std::net::SocketAddr = peer_addr.parse().unwrap();
    let peer_ip = match socket_addr.ip() {
        std::net::IpAddr::V4(v4) => v4.octets(),
        std::net::IpAddr::V6(_)  => [0, 0, 0, 0],
    };
    let peer_port = socket_addr.port();

    {
        let mut s = match state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        s.stream = stream.try_clone().ok();
    }

    let mut my_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            emit_connection("error", &format!("Failed to clone stream: {}", e), &peer_addr);
            return;
        }
    };

    let version_payload = build_version_payload(peer_ip, peer_port);
    if let Err(e) = send_message(&mut my_stream, "version", &version_payload, magic) {
        emit_connection("error", &format!("Failed to send version: {}", e), &peer_addr);
        return;
    }

    let mut message_count: u64 = 0;
    let mut _handshake_done = false;
    let mut _got_their_version = false;

    loop {
        {
            let s = match state.lock() {
                Ok(s) => s,
                Err(_) => break,
            };
            if !s.should_run {
                break;
            }
        }

        let msg = match read_message(&mut my_stream, magic) {
            Ok(m) => m,
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                        let nonce = generate_nonce().to_le_bytes();
                        let _ = send_message(&mut my_stream, "ping", &nonce, magic);
                        continue;
                    }
                    _ => {
                        emit_connection("disconnected", &format!("Disconnected: {}", e), &peer_addr);
                        break;
                    }
                }
            }
        };

        message_count += 1;

        let summary = match msg.command.as_str() {
            "version" => {
                _got_their_version = true;
                let _ = send_message(&mut my_stream, "verack", &[], magic);

                let info_str = decode_version_payload(&msg.payload)
                    .unwrap_or_else(|_| "parse error".to_string());

                if let Ok(info) = parse_version_for_event(&msg.payload) {
                    let _ = app_handle.emit_all("peer-info", info);
                }

                format!("Sent verack. {}", info_str)
            }

            "verack" => {
                _handshake_done = true;
                if let Ok(mut s) = state.lock() {
                    s.is_connected = true;
                }
                emit_connection("handshake_complete",
                    "Version handshake complete — receiving network data",
                    &peer_addr);

                let _ = send_message(&mut my_stream, "getaddr", &[], magic);

                "Handshake complete".to_string()
            }

            "ping" => {
                if msg.payload.len() >= 8 {
                    let nonce = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                    let _ = send_message(&mut my_stream, "pong", &msg.payload[..8], magic);
                    format!("nonce={:016X} → sent pong", nonce)
                } else {
                    "malformed ping".to_string()
                }
            }

            "pong" => {
                if msg.payload.len() >= 8 {
                    let nonce = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                    format!("nonce={:016X}", nonce)
                } else {
                    "received".to_string()
                }
            }

            "inv" => {
                let (count, mut off) = decode_varint(&msg.payload, 0)
                    .unwrap_or((0, 0));

                let is_mempool = {
                    let s = state.lock().unwrap();
                    s.mempool_requested && !s.mempool_done
                };

                if is_mempool {
                    let mut txids: Vec<[u8; 32]> = Vec::new();
                    for _ in 0..count {
                        if off + 36 > msg.payload.len() { break; }
                        let inv_type = u32::from_le_bytes(
                            msg.payload[off..off+4].try_into().unwrap()
                        );
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&msg.payload[off+4..off+36]);
                        off += 36;
                        if inv_type == 1 { txids.push(hash); }
                    }

                    // Emit mempool event to the UI
                    let display_txids: Vec<String> = txids.iter().take(50)
                        .map(|txid| {
                            let mut d = *txid;
                            d.reverse();
                            hex_encode(&d)
                        })
                        .collect();

                    let _ = app_handle.emit_all("mempool-snapshot", MempoolEvent {
                        total_count: txids.len() as u64,
                        txids: display_txids,
                    });

                    // Mark mempool as done
                    if let Ok(mut s) = state.lock() {
                        s.mempool_done = true;
                    }

                    format!("MEMPOOL SNAPSHOT: {} unconfirmed transactions", txids.len())
                } else {
                    let mut items = Vec::new();
                    let mut event_items = Vec::new();
                    let mut local_off = off;
                    
                    for i in 0..count {
                        if local_off + 36 > msg.payload.len() { break; }
                        let inv_type = u32::from_le_bytes(
                            msg.payload[local_off..local_off+4].try_into().unwrap()
                        );
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&msg.payload[local_off+4..local_off+36]);
                        local_off += 36;
                        hash.reverse();
                        
                        let type_str = match inv_type {
                            1 | 0x40000001 => "TX",
                            2 | 0x40000002 => "BLOCK",
                            _ => "OTHER"
                        };
                        
                        let hash_hex = hex_encode(&hash);
                        
                        if i < 5 {
                            items.push(format!("{} {}...", type_str, &hash_hex[..16]));
                        }
                        
                        if event_items.len() < 100 {
                            event_items.push(InvItem {
                                item_type: type_str.to_string(),
                                hash: hash_hex,
                            });
                        }
                    }
                    
                    let _ = app_handle.emit_all("inv-announcement", InvAnnouncement {
                        items: event_items,
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64,
                    });

                    let suffix = if count > 5 {
                        format!(" (+{} more)", count - 5)
                    } else {
                        String::new()
                    };
                    format!("{} item(s): {}{}", count, items.join(", "), suffix)
                }
            }

            "tx" => {
                let mut txid = double_sha256(&msg.payload);
                txid.reverse();
                let txid_str = hex_encode(&txid);

                txid.reverse();
                if let Ok(tx) = parse_transaction(&msg.payload) {
                    let inputs_display: Vec<String> = tx.inputs.iter().map(|inp| {
                        let is_coinbase = inp.prev_txid == [0u8; 32]
                            && inp.prev_index == 0xFFFF_FFFF;
                        if is_coinbase {
                            "COINBASE (block reward)".to_string()
                        } else {
                            let mut d = inp.prev_txid;
                            d.reverse();
                            format!("Spends {}...  output #{}", &hex_encode(&d)[..16], inp.prev_index)
                        }
                    }).collect();

                    let mut outputs_display: Vec<String> = Vec::new();
                    let mut total_sats: u64 = 0;
                    for out in &tx.outputs {
                        total_sats += out.value;
                        let btc = out.value as f64 / 100_000_000.0;
                        let script = classify_script_simple(&out.script_pubkey);
                        outputs_display.push(format!("{:.8} BTC → {}", btc, script));
                    }

                    let locktime_str = match tx.locktime {
                        0 => "0 (no restriction)".to_string(),
                        n if n < 500_000_000 => format!("block {}", n),
                        n => format!("unix {}", n),
                    };

                    txid.reverse();
                    let _ = app_handle.emit_all("transaction-decoded", TransactionEvent {
                        txid: hex_encode(&txid),
                        fetched_from: peer_addr.clone(),
                        version: tx.version,
                        is_segwit: tx.is_segwit,
                        input_count: tx.inputs.len(),
                        output_count: tx.outputs.len(),
                        total_output_sats: total_sats,
                        outputs: outputs_display,
                        inputs: inputs_display,
                        locktime: locktime_str,
                    });
                }

                format!("txid={}...", &txid_str[..16])
            }

            "addr" => {
                let (count, _) = decode_varint(&msg.payload, 0).unwrap_or((0,0));
                format!("{} peer addresses", count)
            }

            "headers" => {
                let (count, _) = decode_varint(&msg.payload, 0).unwrap_or((0,0));
                format!("{} block headers", count)
            }

            "block" => {
                if msg.payload.len() >= 80 {
                    let mut h = double_sha256(&msg.payload[..80]);
                    h.reverse();
                    format!("hash={}...", &hex_encode(&h)[..16])
                } else {
                    "truncated".to_string()
                }
            }

            "feefilter" => {
                if msg.payload.len() >= 8 {
                    let fee = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                    format!("{} sat/kB minimum", fee)
                } else { "received".to_string() }
            }

            "sendheaders"  => "peer prefers header announcements".to_string(),
            "sendcmpct"    => "peer supports compact blocks".to_string(),
            "getheaders"   => "ignoring (no headers to serve)".to_string(),
            "getdata"      => "ignoring (no data to serve)".to_string(),
            "reject"       => format!("{} bytes", msg.payload.len()),
            "notfound"     => {
                let _ = app_handle.emit_all("transaction-notfound", ());
                let (count, _) = decode_varint(&msg.payload, 0).unwrap_or((0,0));
                format!("peer does not have {} requested item(s)", count)
            },
            _other          => format!("({} bytes payload)", msg.payload.len()),
        };

        // Emit message event for the live log
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let _ = app_handle.emit_all("bitcoin-message", MessageEvent {
            command: msg.command.clone(),
            summary,
            timestamp: ts,
            message_number: message_count,
        });
    }

    if let Ok(mut s) = state.lock() {
        s.is_connected = false;
        s.should_run = false;
        s.stream = None;
    }
}

// Helpers

fn parse_version_for_event(payload: &[u8]) -> Result<PeerInfoEvent, ()> {
    if payload.len() < 81 { return Err(()); }
    let version = i32::from_le_bytes(payload[0..4].try_into().map_err(|_| ())?);
    let services = u64::from_le_bytes(payload[4..12].try_into().map_err(|_| ())?);
    let mut offset = 20 + 52 + 8; // skip timestamp + addrs + nonce
    let (ua_len, vs) = decode_varint(payload, offset).map_err(|_| ())?;
    offset += vs;
    let ua_end = (offset + ua_len as usize).min(payload.len());
    let user_agent = String::from_utf8_lossy(&payload[offset..ua_end]).to_string();
    let start_height = if ua_end + 4 <= payload.len() {
        i32::from_le_bytes(payload[ua_end..ua_end+4].try_into().map_err(|_| ())?)
    } else { 0 };
    Ok(PeerInfoEvent {
        protocol_version: version,
        user_agent,
        start_height,
        services: format!("0x{:016X}", services),
    })
}

fn classify_script_simple(script: &[u8]) -> String {
    match script {
        s if s.len() == 25 && s[0] == 0x76 && s[1] == 0xa9 => "P2PKH".to_string(),
        s if s.len() == 23 && s[0] == 0xa9 && s[22] == 0x87 => "P2SH".to_string(),
        s if s.len() == 22 && s[0] == 0x00 && s[1] == 0x14  => "P2WPKH".to_string(),
        s if s.len() == 34 && s[0] == 0x00 && s[1] == 0x20  => "P2WSH".to_string(),
        s if !s.is_empty() && s[0] == 0x6a => "OP_RETURN".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}
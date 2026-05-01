// src/node.rs — Background Bitcoin P2P thread and shared event types
//
// This module has two jobs:
//
//   1. Define the data structures (events) that flow from Rust → JavaScript.
//      These are serialized to JSON by serde and sent across the Tauri bridge.
//
//   2. Run the Bitcoin P2P connection on a background thread so it never
//      blocks Tauri's main thread (which owns the UI window).
//
// Why a background thread?
//   Your Bitcoin message loop calls read_message() which blocks waiting for
//   data from the TCP socket. If this ran on the main thread, the entire
//   window would freeze. By spawning a separate OS thread, the UI stays
//   responsive while Bitcoin network I/O happens in the background.

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
use crate::transaction::{parse_transaction, decode_and_display_tx, request_transaction};
use crate::mempool;

// ── Event structs ─────────────────────────────────────────────────────────────
//
// Each struct below represents one type of event we emit from Rust to JS.
// #[derive(Serialize)] generates the JSON serialization automatically:
//   struct MessageEvent { command: "ping", summary: "nonce=ABC" }
//   becomes: {"command":"ping","summary":"nonce=ABC"}
//
// #[derive(Clone)] is needed because Tauri's emit_all() takes ownership,
// so we need to be able to clone the struct when emitting.
//
// The #[serde(rename_all = "camelCase")] attribute converts Rust's snake_case
// field names to JavaScript's camelCase convention:
//   peer_address → peerAddress
//   is_connected → isConnected

// Emitted when our connection status changes (connecting, connected, error, etc.)
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    // "connecting" | "connected" | "handshake_complete" | "disconnected" | "error"
    pub status: String,
    // Human-readable detail about this status change
    pub message: String,
    // The peer's IP:port, empty string if not yet connected
    pub peer_address: String,
}

// Emitted for every Bitcoin P2P message we receive from the peer.
// This populates the live message log panel in the UI.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MessageEvent {
    // The message command: "version", "inv", "ping", "tx", etc.
    pub command: String,
    // A one-line human-readable summary of the message content
    pub summary: String,
    // Unix timestamp in milliseconds (JS uses ms, not seconds)
    pub timestamp: u64,
    // Running count of messages received (for display numbering)
    pub message_number: u64,
}

// Emitted when we receive peer info from a `version` message.
// This populates the "Peer Info" panel in the UI.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfoEvent {
    pub protocol_version: i32,
    // The peer's software name, e.g. "/Satoshi:25.0.0/"
    pub user_agent: String,
    // The peer's best block height (how synced they are)
    pub start_height: i32,
    // Services bitmask as hex string, e.g. "0x0000000000000409"
    pub services: String,
}

// Emitted when we decode a full transaction.
// This populates the "Transaction Detail" panel in the UI.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TransactionEvent {
    // The txid in display order (reversed bytes, hex encoded)
    pub txid: String,
    // The node IP we fetched this from
    pub fetched_from: String,
    pub version: i32,
    pub is_segwit: bool,
    pub input_count: usize,
    pub output_count: usize,
    // Total value of all outputs in satoshis
    pub total_output_sats: u64,
    // Each output as a string: "0.00450000 BTC → P2WPKH hash=ab12..."
    pub outputs: Vec<String>,
    // Each input as a string: "Spends tx:9f8e... output #0"
    pub inputs: Vec<String>,
    // Locktime decoded as a human string
    pub locktime: String,
}

// Emitted when we receive a mempool `inv` response.
// This populates the "Mempool" panel in the UI.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MempoolEvent {
    // How many unconfirmed transactions the peer reported
    pub total_count: u64,
    // The first N txids (in display order) for showing in the table
    pub txids: Vec<String>,
}

// ── Shared application state ──────────────────────────────────────────────────
//
// This struct holds state that needs to be accessed from multiple places:
//   - The background thread reads/writes it
//   - Tauri commands (from JS) read/write it
//
// We wrap it in Arc<Mutex<...>> so it can be safely shared across threads.
//   Arc  = "Atomic Reference Count" — lets multiple owners share one value
//   Mutex = ensures only one thread accesses it at a time (mutual exclusion)
//
// Pattern: Arc<Mutex<T>> is the standard Rust way to share mutable state
// between threads. You'll see this everywhere in concurrent Rust code.
pub struct NodeState {
    // The live TCP stream, if connected. None if not connected.
    // Wrapped in Option because we might not have a connection yet.
    pub stream: Option<TcpStream>,
    // Whether we're currently connected and past the handshake
    pub is_connected: bool,
    // Whether the background thread should keep running.
    // Setting this to false causes the loop to exit and the thread to end.
    pub should_run: bool,
    // Whether we've sent a mempool request and are waiting for the inv response
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

// Type alias for our shared state — avoids typing Arc<Mutex<NodeState>> everywhere
pub type SharedState = Arc<Mutex<NodeState>>;

// ── Background thread function ────────────────────────────────────────────────
//
// This is the entry point for the background thread.
// It's called from commands.rs when JS invokes the "connect" command.
//
// Parameters:
//   app_handle — Tauri's handle for emitting events to the frontend.
//                Cloneable and Send-safe — fine to pass into threads.
//   state      — shared mutable state (Arc<Mutex<NodeState>>)
//
// This function:
//   1. Discovers a testnet peer
//   2. Opens a TCP connection
//   3. Sends the version message
//   4. Loops reading messages, emitting events for each one
//   5. Exits cleanly when state.should_run becomes false
pub fn run_bitcoin_node(app_handle: AppHandle, state: SharedState) {
    let magic = MAGIC_TESTNET;

    // ── Emit helper closure ───────────────────────────────────────────────
    // Instead of calling app_handle.emit_all(...) everywhere (verbose),
    // we define a small closure that does it in one line.
    // A closure is like an inline function that captures variables from its scope.
    let emit_connection = |status: &str, message: &str, peer: &str| {
        let _ = app_handle.emit_all("connection-status", ConnectionEvent {
            status: status.to_string(),
            message: message.to_string(),
            peer_address: peer.to_string(),
        });
    };

    // ── Step 1: Peer discovery ────────────────────────────────────────────
    emit_connection("connecting", "Resolving DNS seeds...", "");

    let mut candidates = find_testnet_peers();
    if candidates.is_empty() {
        candidates = vec![
            "185.210.125.33:18333".to_string(),
            "77.163.221.171:18333".to_string(),
        ];
    }

    // ── Step 2: TCP connection ────────────────────────────────────────────
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

    // Store the stream in shared state so commands.rs can write to it
    // (e.g. when JS sends a "request transaction" command)
    let socket_addr: std::net::SocketAddr = peer_addr.parse().unwrap();
    let peer_ip = match socket_addr.ip() {
        std::net::IpAddr::V4(v4) => v4.octets(),
        std::net::IpAddr::V6(_)  => [0, 0, 0, 0],
    };
    let peer_port = socket_addr.port();

    // ── Step 3: Send version ──────────────────────────────────────────────
    {
        // Scope the mutex lock so it's released before we enter the read loop
        let mut s = match state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        // Clone the stream so we can store one copy and use another
        // TcpStream::try_clone() creates a second handle to the same socket
        s.stream = stream.try_clone().ok();
    }

    // We need our own clone for this thread to read from
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

    // ── Step 4: Message loop ──────────────────────────────────────────────
    let mut message_count: u64 = 0;
    let mut handshake_done = false;
    let mut got_their_version = false;

    loop {
        // Check if we should stop (set by the "disconnect" command)
        {
            let s = match state.lock() {
                Ok(s) => s,
                Err(_) => break,
            };
            if !s.should_run {
                break;
            }
        }

        // Read one complete message from the socket
        let msg = match read_message(&mut my_stream, magic) {
            Ok(m) => m,
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                        // Timeout — send a ping to stay alive
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

        // ── Dispatch each message type ────────────────────────────────────
        // For each message we:
        //   a) Do any protocol-level response (like sending verack or pong)
        //   b) Build a human-readable summary string
        //   c) If relevant, emit a specific event (peer info, tx, mempool)
        //   d) Always emit a MessageEvent for the live log
        let summary = match msg.command.as_str() {

            "version" => {
                got_their_version = true;
                // Send verack immediately
                let _ = send_message(&mut my_stream, "verack", &[], magic);

                // Parse the version payload to extract peer info
                let info_str = decode_version_payload(&msg.payload)
                    .unwrap_or_else(|_| "parse error".to_string());

                // Also emit a structured PeerInfoEvent so the UI can
                // populate the peer info panel with individual fields
                if let Ok(info) = parse_version_for_event(&msg.payload) {
                    let _ = app_handle.emit_all("peer-info", info);
                }

                format!("Sent verack. {}", info_str)
            }

            "verack" => {
                handshake_done = true;
                if let Ok(mut s) = state.lock() {
                    s.is_connected = true;
                }
                emit_connection("handshake_complete",
                    "Version handshake complete — receiving network data",
                    &peer_addr);

                // After handshake, ask for peer addresses
                let _ = send_message(&mut my_stream, "getaddr", &[], magic);

                "Handshake complete".to_string()
            }

            "ping" => {
                if msg.payload.len() >= 8 {
                    let nonce = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                    // Echo the nonce back as pong
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
                // Parse the inv to count items and build summary
                let (count, mut off) = decode_varint(&msg.payload, 0)
                    .unwrap_or((0, 0));

                // Check if this is a mempool response
                let is_mempool = {
                    let s = state.lock().unwrap();
                    s.mempool_requested && !s.mempool_done
                };

                if is_mempool {
                    // Collect all txids from this inv
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
                    // Regular live inv — collect and display items
                    let mut items = Vec::new();
                    let mut local_off = off;
                    for _ in 0..count.min(5) {
                        if local_off + 36 > msg.payload.len() { break; }
                        let inv_type = u32::from_le_bytes(
                            msg.payload[local_off..local_off+4].try_into().unwrap()
                        );
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&msg.payload[local_off+4..local_off+36]);
                        local_off += 36;
                        hash.reverse();
                        let type_str = match inv_type {
                            1 => "TX", 2 => "BLOCK", _ => "OTHER"
                        };
                        items.push(format!("{} {}...", type_str, &hex_encode(&hash)[..16]));
                    }
                    let suffix = if count > 5 {
                        format!(" (+{} more)", count - 5)
                    } else {
                        String::new()
                    };
                    format!("{} item(s): {}{}", count, items.join(", "), suffix)
                }
            }

            "tx" => {
                // Compute the txid (double-SHA256 of raw tx, reversed for display)
                let mut txid = double_sha256(&msg.payload);
                txid.reverse();
                let txid_str = hex_encode(&txid);

                // Parse the transaction and emit a structured event for the UI
                txid.reverse(); // un-reverse for internal use in parse
                if let Ok(tx) = parse_transaction(&msg.payload) {
                    let mut inputs_display: Vec<String> = tx.inputs.iter().map(|inp| {
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

                    // Reverse txid back for display
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
            other          => format!("({} bytes payload)", msg.payload.len()),
        };

        // ── Always emit a MessageEvent for the live log ───────────────────
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

    // Thread is exiting — update state
    if let Ok(mut s) = state.lock() {
        s.is_connected = false;
        s.should_run = false;
        s.stream = None;
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Parse a version payload into a PeerInfoEvent struct.
// Returns Err if the payload is too short.
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

// Simplified script classifier for the UI (shorter output than transaction.rs)
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
// After the handshake completes (version + verack exchanged both ways),
// the peer will begin sending us real Bitcoin network messages.
//
//
//   network.rs     ← DNS seed resolution, peer discovery
//   peer.rs        ← TCP connect, send_message, handle_message dispatcher
//   parser.rs      ← reads raw bytes from socket into BitcoinMessage structs
//   message.rs     ← MessageHeader, build_message, magic constants
//   version.rs     ← builds/decodes the version handshake payload
//   encoding.rs    ← varint, varstr, net_addr encode/decode helpers
//   crypto.rs      ← SHA256 / double_sha256 / hex_encode
//   transaction.rs ← request + decode full transactions via getdata/tx
//   mempool.rs     ← request peer's mempool via mempool/inv messages

mod crypto;
mod encoding;
mod message;
mod version;
mod parser;
mod peer;
mod network;
mod transaction; 
mod mempool;    

use std::io;
use message::MAGIC_TESTNET;
use version::{build_version_payload, generate_nonce};
use peer::{connect_to_peer, send_message, handle_message};
use parser::read_message;
use network::find_testnet_peers;

// Helper to decode a hex string into a 32-byte array, reversed for internal use.
// This is needed because users input TXIDs in display order (like block explorers),
// but the Bitcoin protocol requires them in internal byte order (reversed).
fn parse_txid_hex(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 {
        return Err("TXID must be exactly 64 hex characters".to_string());
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        let byte_str = &hex[i * 2..i * 2 + 2];
        bytes[i] = u8::from_str_radix(byte_str, 16)
            .map_err(|_| "Invalid hex character in TXID".to_string())?;
    }
    bytes.reverse(); // Convert display order to internal byte order
    Ok(bytes)
}

fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║   Rust Bitcoin P2P Client — Testnet3     ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    // Command Line Arguments
    let args: Vec<String> = std::env::args().collect();
    let mut request_mempool_on_start = false;
    let mut target_txid: Option<String> = None;

    if args.len() > 1 {
        let arg = &args[1];
        if arg == "mempool" {
            request_mempool_on_start = true;
            println!("[*] CLI flag: Will request mempool snapshot after handshake.");
            println!("    (Note: some peers may disconnect when you ask for this)");
        } else if arg.len() == 64 {
            target_txid = Some(arg.clone());
            println!("[*] CLI flag: Will request specific TX: {}", arg);
        } else {
            eprintln!("Usage: cargo run -- [mempool | <txid>]");
            std::process::exit(1);
        }
    }
    println!();

    // Step 1: Peer Discovery
    // Query all testnet DNS seeds and collect every IP they return.
    // We'll iterate through them in order, attempting a TCP connection
    // to each one with a 5-second timeout, stopping at the first success.
    let mut candidates = find_testnet_peers();

    // Last-resort fallback addresses — used only if ALL DNS seeds fail.
    // These are stable, long-running testnet nodes maintained by the community.
    if candidates.is_empty() {
        println!("[!] All DNS seeds failed — using hardcoded fallback addresses");
        candidates = vec![
            "185.210.125.33:18333".to_string(),
            "77.163.221.171:18333".to_string(),
            "69.59.18.23:18333".to_string(),
            "193.30.123.70:18333".to_string(),
        ];
    }

    // ── Step 2: TCP Connection ────────────────────────────────────────────
    // Try each candidate in turn. connect_to_peer uses a 5-second timeout,
    // so dead/firewalled peers fail quickly instead of hanging for ~75s.
    println!("[*] Trying {} candidate peer(s)...\n", candidates.len());

    let (mut stream, peer_addr) = 'connect: {
        for addr in &candidates {
            match connect_to_peer(addr) {
                Ok(s) => {
                    println!("[+] Connected to {}\n", addr);
                    break 'connect (s, addr.clone());
                }
                Err(e) => {
                    println!("[!] {} — {} (trying next...)", addr, e);
                }
            }
        }
        // All candidates exhausted — nothing worked.
        eprintln!("[!] Could not connect to any peer. Check your internet connection.");
        std::process::exit(1);
    };

    // Parse the peer's IP and port from the address string.
    // We need these as separate values for the version message payload.
    let socket_addr: std::net::SocketAddr = peer_addr.parse().unwrap();
    let peer_ip = match socket_addr.ip() {
        std::net::IpAddr::V4(ipv4) => ipv4.octets(), // [a, b, c, d]
        std::net::IpAddr::V6(_)    => [0, 0, 0, 0],  // Simplified: zero out IPv6 peers
    };
    let peer_port = socket_addr.port();

    // The network magic bytes we expect on ALL messages from this peer.
    // Using testnet magic means any mainnet messages will be rejected.
    let magic = MAGIC_TESTNET;

    // Step 3: Send Version Message
    // Bitcoin protocol rule: the connecting party (us) MUST send `version` first.
    // The receiving party (the peer) will then send their `version` back,
    // followed by `verack`. We send our `verack` when we receive theirs.
    let version_payload = build_version_payload(peer_ip, peer_port);
    if let Err(e) = send_message(&mut stream, "version", &version_payload, magic) {
        eprintln!("[!] Failed to send version message: {}", e);
        std::process::exit(1);
    }
    println!("[→] Version message sent — waiting for peer response...\n");

    //  Step 4: Message Loop
    // Track the handshake state. Both flags must be true before the peer
    // will send us useful data.
    let mut handshake_done    = false; // True after we receive their verack
    let mut got_their_version = false; // True after we receive their version
    let mut message_count     = 0u64; // Total messages received (for display)

    //  Mempool and transaction tracking flags
    //
    // mempool_requested: becomes true when we send the `mempool` message.
    //   We use this flag so that when an `inv` arrives, handle_message knows
    //   whether to treat it as a mempool response (potentially hundreds of txids)
    //   or as a live new-transaction announcement (usually 1-3 txids).
    //   Without this flag, we can't tell the difference just from the message itself.
    //
    // mempool_done: becomes true after we've processed the mempool inv response.
    //   Prevents us from treating subsequent `inv` messages as mempool responses.
    let mut mempool_requested = false;
    let mut mempool_done      = false;

    println!("[*] Entering message loop... (press Ctrl+C to stop)\n");

    loop {
        // Block until a complete Bitcoin message arrives from the peer.
        // read_message handles: exact byte reads, magic validation, checksum validation.
        let msg = match read_message(&mut stream, magic) {
            Ok(m) => m,

            Err(e) => match e.kind() {
                // Read timeout: no data received within the 30-second window we set.
                // This is normal — the network is quiet right now. Send a ping to:
                //   a) Keep the connection alive (peer won't disconnect idle peers)
                //   b) Verify the peer is still responsive
                io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
                    println!("[*] No data for 30s — sending keepalive ping");
                    let nonce_bytes = generate_nonce().to_le_bytes();
                    if let Err(e) = send_message(&mut stream, "ping", &nonce_bytes, magic) {
                        eprintln!("[!] Failed to send ping: {}", e);
                        break; // Connection is dead
                    }
                    continue; // Go back to waiting for the next message
                }

                // The peer closed the connection gracefully (sent TCP FIN).
                // This is normal — peers disconnect idle observers after a while.
                io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset => {
                    println!("[!] Peer disconnected (connection closed).");
                    break;
                }

                // Any other I/O error (network failure, OS error, etc.)
                _ => {
                    eprintln!("[!] Fatal read error: {} — disconnecting", e);
                    break;
                }
            }
        };

        // ── Message received — process it ─────────────────────────────────
        message_count += 1;
        // Print a visual separator for each message to make the log readable
        println!("\n[MSG #{:04}] ─────────────────────────────────────", message_count);

        // ── Dispatch to handle_message ────────────────────────────────────
        //
        // handle_message (in peer.rs) is the central dispatcher — it matches
        // on msg.command and routes to the right handler for each message type.
        //
        // We pass mempool_requested so the `inv` handler knows whether to
        // treat an incoming inv as a mempool dump or a live tx announcement.
        if let Err(e) = handle_message(
            &msg,
            &mut stream,
            magic,
            &mut handshake_done,
            &mut got_their_version,
            mempool_requested && !mempool_done, // is this inv a mempool response?
        ) {
            eprintln!("[!] Error handling '{}' message: {}", msg.command, e);
        }

        // ── Mark mempool response as handled ──────────────────────────────
        if mempool_requested && !mempool_done && msg.command == "inv" {
            mempool_done = true;
            mempool::print_mempool_summary(&[]); // print closing summary line
        }

        // ── Post-handshake actions (run once each) ────────────────────────
        //
        // These blocks use message_count as a simple "run once" trigger.
        // We offset them so they don't all fire at the same time.

        // At message 5: request peer addresses (getaddr)
        // The peer responds with an `addr` message containing up to 1000
        // IP addresses of other nodes it knows about.
        if handshake_done && message_count == 5 {
            println!("\n[*] Handshake complete — requesting peer addresses (getaddr)...");
            if let Err(e) = send_message(&mut stream, "getaddr", &[], magic) {
                eprintln!("[!] Failed to send getaddr: {}", e);
            }
        }

        // At message 6: Process Command Line Arguments
        // We wait until message 6 (after getaddr) so the peer has had a chance
        // to settle after the handshake before we fire another request.
        if handshake_done && message_count == 6 {
            // Check if user requested a specific TX on the command line
            if let Some(ref hex_txid) = target_txid {
                println!("\n[*] Requesting specific TX from command line...");
                match parse_txid_hex(hex_txid) {
                    Ok(internal_txid) => {
                        if let Err(e) = transaction::request_transaction(&mut stream, &internal_txid, magic) {
                            eprintln!("[!] Failed to request transaction: {}", e);
                        }
                    }
                    Err(e) => eprintln!("[!] Invalid TXID format: {}", e),
                }
            }
            // Or check if user requested a mempool snapshot
            else if request_mempool_on_start && !mempool_requested {
                if let Err(e) = mempool::request_mempool_snapshot(&mut stream, magic) {
                    eprintln!("[!] Failed to send mempool request: {}", e);
                } else {
                    mempool_requested = true; // flag: the next inv is a mempool response
                }
            }
        }
    }

    // ── Session ended ─────────────────────────────────────────────────────
    println!("\n[*] Session ended. Total messages received: {}", message_count);
    println!("[*] Goodbye.");
}
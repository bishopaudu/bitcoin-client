// After the handshake completes (version + verack exchanged both ways),
// the peer will begin sending us real Bitcoin network messages:

mod crypto;   // SHA256 implementation and hex encoding
mod encoding; // varint, varstr, net_addr encode/decode
mod message;  // MessageHeader struct, build_message, magic constants
mod version;  // version payload builder and decoder
mod parser;   // streaming message reader (read_message)
mod peer;     // TCP connection, send_message, message dispatcher
mod network;  // DNS seed resolution

use std::io;
use message::MAGIC_TESTNET;
use version::{build_version_payload, generate_nonce};
use peer::{connect_to_peer, send_message, handle_message};
use parser::read_message;
use network::find_testnet_peers;

fn main() {
    println!("╔══════════════════════════════════════════╗");
    println!("║   Rust Bitcoin P2P Client — Testnet3     ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    // ── Step 1: Peer Discovery ────────────────────────────────────────────
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
                    // Print a short reason and move on to the next candidate.
                    // Typical reasons: "timed out", "connection refused", "network unreachable"
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

    // ── Step 3: Send Version Message ─────────────────────────────────────
    // Bitcoin protocol rule: the connecting party (us) MUST send `version` first.
    // The receiving party (the peer) will then send their `version` back,
    // followed by `verack`. We send our `verack` when we receive theirs.
    let version_payload = build_version_payload(peer_ip, peer_port);
    if let Err(e) = send_message(&mut stream, "version", &version_payload, magic) {
        eprintln!("[!] Failed to send version message: {}", e);
        std::process::exit(1);
    }
    println!("[→] Version message sent — waiting for peer response...\n");

    // ── Step 4: Message Loop ──────────────────────────────────────────────
    // Track the handshake state. Both flags must be true before the peer
    // will send us useful data.
    let mut handshake_done    = false; // True after we receive their verack
    let mut got_their_version = false; // True after we receive their version
    let mut message_count     = 0u64; // Total messages received (for display)

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

        // Dispatch the message to the appropriate handler in peer.rs.
        // The handler updates handshake_done / got_their_version as a side effect.
        if let Err(e) = handle_message(
            &msg,
            &mut stream,
            magic,
            &mut handshake_done,
            &mut got_their_version,
        ) {
            eprintln!("[!] Error handling '{}' message: {}", msg.command, e);
            // Don't break — a single handler error shouldn't kill the connection
        }

        // ── Post-handshake: request more peer addresses once ───────────────
        // After the handshake completes, send `getaddr` to request a list of
        // peers this node knows about. The peer will respond with an `addr`
        // message containing up to 1000 IP addresses. We do this once (at
        // message 5) to avoid spamming the peer with repeated requests.
        if handshake_done && message_count == 5 {
            println!("\n[*] Handshake complete — requesting peer addresses (getaddr)...");
            if let Err(e) = send_message(&mut stream, "getaddr", &[], magic) {
                eprintln!("[!] Failed to send getaddr: {}", e);
            }
        }
    }

    // ── Session ended ─────────────────────────────────────────────────────
    println!("\n[*] Session ended. Total messages received: {}", message_count);
    println!("[*] Goodbye.");
}
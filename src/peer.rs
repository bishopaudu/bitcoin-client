// peer.rs — Peer connection management and message handling
//
// This module owns everything related to a single peer connection:
//   - Opening the TCP socket with the right settings
//   - Sending messages down the socket
//   - Handling each incoming message type after it's been parsed
//   - Maintaining handshake state
//
// The message handler here is a dispatcher: it matches on the command
// string and routes each message to the appropriate logic. For a more
// advanced client you'd split each handler into its own function or module.

use std::net::{TcpStream, ToSocketAddrs};
use std::io::{self, Write};
use std::time::Duration;
use crate::message::build_message;
use crate::version::{build_version_payload, decode_version_payload, generate_nonce};
use crate::parser::{read_message, BitcoinMessage};
use crate::encoding::decode_varint;
use crate::crypto::{double_sha256, hex_encode};

// Open a TCP connection to a Bitcoin peer and configure the socket.
//
// `addr`: a "ip:port" string, e.g. "203.0.113.1:18333"
//
// Socket configuration:
//   read_timeout  — prevents blocking forever if the peer stops sending
//   write_timeout — prevents hanging if the peer stops reading
//   nodelay       — disables Nagle's algorithm (send bytes immediately,
//                   don't buffer small writes — latency > bandwidth here)
pub fn connect_to_peer(addr: &str) -> io::Result<TcpStream> {
    println!("[*] Connecting to Bitcoin node at {}...", addr);

    // Resolve the address to a SocketAddr — required by connect_timeout.
    let socket_addr = addr
        .to_socket_addrs()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no addresses resolved"))?;

    // connect_timeout replaces the plain connect() call. The OS default TCP
    // connect timeout is ~75 seconds — far too long when iterating through a
    // list of candidates. 5 seconds is enough to confirm a peer is reachable.
    let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5))?;

    println!("[+] TCP connection established to {}", addr);

    // If we don't receive any data for 30 seconds, return a TimedOut error
    // rather than blocking forever. We'll use this to send keepalive pings.
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;

    // If a write blocks for 30 seconds (e.g. peer's receive buffer is full),
    // return an error rather than hanging indefinitely.
    stream.set_write_timeout(Some(Duration::from_secs(60)))?;

    // Disable Nagle's algorithm. Nagle buffers small writes hoping to batch them
    // into larger TCP segments for efficiency. For Bitcoin's protocol, we want
    // each message sent immediately — a delayed `verack` would stall the handshake.
    stream.set_nodelay(true)?;

    Ok(stream)
}

// Serialize and send a Bitcoin message over the TCP connection.
//
// `command`: the message type, e.g. "version", "verack", "ping", "getaddr"
// `payload`: the raw payload bytes (empty slice for messages with no body)
// `magic`:   network identifier (use MAGIC_TESTNET or MAGIC_MAINNET)
//
// We use `write_all` rather than `write` because `write` is allowed to send
// fewer bytes than requested (partial write). `write_all` loops until every
// byte has been handed to the OS, guaranteeing complete message delivery.
pub fn send_message(
    stream: &mut TcpStream,
    command: &str,
    payload: &[u8],
    magic: [u8; 4],
) -> io::Result<()> {
    // build_message combines the 24-byte header (with computed checksum)
    // and the payload into a single contiguous byte vector
    let message = build_message(command, payload, magic);

    println!(
        "[→] Sending '{}' ({} bytes payload, {} bytes total)",
        command,
        payload.len(),
        message.len()
    );

    // Write every byte of the message to the socket — no partial writes
    stream.write_all(&message)?;

    // Flush ensures the bytes leave our userspace buffer and enter the OS
    // network stack. Without this, writes might be held in a buffer.
    stream.flush()?;

    Ok(())
}

// Send a `verack` message — the acknowledgement half of the handshake.
//
// `verack` has zero payload. It simply means:
// "I received your version message and accept it. Handshake is complete on my side."
// We must send this after receiving the peer's `version` message.
pub fn send_verack(stream: &mut TcpStream, magic: [u8; 4]) -> io::Result<()> {
    send_message(stream, "verack", &[], magic) // Empty payload slice
}

// Dispatch an incoming Bitcoin message to the appropriate handler.
//
// This function is called once for every message received from the peer.
// It matches on the command string and takes appropriate action:
//   - During handshake: respond to version/verack
//   - Keepalive: respond to ping with pong
//   - Observing: log inv, addr, block, tx, etc.
//
// `handshake_done`:    set to true once we've received their verack
// `got_their_version`: set to true once we've received their version
pub fn handle_message(
    msg: &BitcoinMessage,
    stream: &mut TcpStream,
    magic: [u8; 4],
    handshake_done: &mut bool,
    got_their_version: &mut bool,
) -> io::Result<()> {
    match msg.command.as_str() {

        // ── version ───────────────────────────────────────────────────────
        // The remote peer is introducing themselves. This is the first
        // message we expect to receive. We must respond with `verack`.
        // Note: the peer also waits for OUR version (which we sent first)
        // and will send us their verack once they receive it.
        "version" => {
            match decode_version_payload(&msg.payload) {
                Ok(info) => println!("[←] VERSION: {}", info),
                Err(e)   => println!("[←] VERSION (could not parse: {})", e),
            }
            *got_their_version = true;
            // Immediately send our acknowledgement — the protocol requires this
            send_verack(stream, magic)?;
            println!("[→] Sent verack");
        }

        // ── verack ────────────────────────────────────────────────────────
        // The peer acknowledges OUR version message. Once we have sent verack
        // AND received verack, the handshake is complete and real messages flow.
        // Bitcoin Core will start sending `sendheaders`, `sendcmpct`,
        // `feefilter`, `ping`, and eventually `inv` messages after this.
        "verack" => {
            println!("[←] VERACK — handshake complete! Ready to receive network data.");
            *handshake_done = true;
        }

        // ── ping ──────────────────────────────────────────────────────────
        // A keepalive probe. The peer sends a random 8-byte nonce and
        // expects us to echo it back in a `pong` message within ~20 minutes.
        // Failure to respond will cause the peer to disconnect us.
        // This is how Bitcoin nodes detect dead/unresponsive peers.
        "ping" => {
            if msg.payload.len() >= 8 {
                let nonce = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                println!("[←] PING nonce={:016X} — responding with pong", nonce);
                // Echo the exact same nonce back — this is the pong payload
                send_message(stream, "pong", &msg.payload[..8], magic)?;
            } else {
                println!("[←] PING (malformed — payload too short)");
            }
        }

        // ── pong ──────────────────────────────────────────────────────────
        // The peer's response to a ping WE sent. Contains the same nonce
        // we put in our ping. We log it but don't need to act on it.
        "pong" => {
            if msg.payload.len() >= 8 {
                let nonce = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                println!("[←] PONG nonce={:016X}", nonce);
            }
        }

        // ── inv ───────────────────────────────────────────────────────────
        // "Inventory" — the peer is announcing things it has available.
        // This is the primary mechanism for transaction and block propagation:
        //
        //   1. Node A mines a block → sends `inv` to all its peers
        //   2. Peers that don't have it send back `getdata`
        //   3. Node A responds with the full `block` message
        //
        // inv entries are 36 bytes each:
        //   type (4 bytes): 1=TX, 2=BLOCK, 3=FILTERED_BLOCK, 4=CMPCT_BLOCK
        //   hash (32 bytes): the txid or block hash
        "inv" => {
            parse_and_log_inv(&msg.payload);
        }

        // ── addr ──────────────────────────────────────────────────────────
        // A list of other Bitcoin peers this node knows about.
        // Each entry is 30 bytes: 4-byte timestamp + 26-byte net_addr.
        // This is how the peer-to-peer network self-organizes and heals —
        // nodes gossip peer addresses so new nodes can discover the network.
        // A crawler would save these addresses and connect to them too.
        "addr" => {
            if let Ok((count, _)) = decode_varint(&msg.payload, 0) {
                println!("[←] ADDR — peer shared {} network addresses", count);
            }
        }

        // ── headers ───────────────────────────────────────────────────────
        // A batch of block headers (80 bytes each, without transactions).
        // Used in the "headers-first" sync mode: download all headers first
        // to verify proof-of-work, then download full blocks in parallel.
        // SPV (lightweight) clients only ever download headers, not full blocks.
        "headers" => {
            if let Ok((count, _)) = decode_varint(&msg.payload, 0) {
                println!("[←] HEADERS — {} block headers received", count);
            }
        }

        // ── block ─────────────────────────────────────────────────────────
        // A full block: 80-byte header followed by all transactions.
        // Can be up to 4MB with SegWit witness data.
        // The block "hash" (its ID) is double-SHA256 of just the 80-byte header.
        // Bitcoin displays block hashes in REVERSED byte order (display convention).
        "block" => {
            if msg.payload.len() >= 80 {
                // Compute the block hash from just the 80-byte header portion
                let mut h = double_sha256(&msg.payload[..80]);
                // Reverse for display: Bitcoin shows hashes in reversed byte order
                h.reverse();
                println!("[←] BLOCK hash={}", hex_encode(&h));
            } else {
                println!("[←] BLOCK (payload too short to parse header)");
            }
        }

        // ── tx ────────────────────────────────────────────────────────────
        // A raw serialized transaction. The txid is double-SHA256 of the
        // entire serialized transaction bytes (the whole payload here).
        // Like block hashes, txids are displayed in reversed byte order.
        "tx" => {
            // The txid is the double-SHA256 of the raw transaction bytes
            let mut txid = double_sha256(&msg.payload);
            txid.reverse(); // Reverse for display convention
            println!("[←] TX txid={}", hex_encode(&txid));
        }

        // ── feefilter ─────────────────────────────────────────────────────
        // Introduced in Bitcoin Core 0.13.0 (BIP 133).
        // The peer is telling us: "only send me transactions with a fee rate
        // above this threshold (in satoshis per kilobyte)."
        // This saves bandwidth — low-fee transactions are filtered at the relay level.
        "feefilter" => {
            if msg.payload.len() >= 8 {
                let fee = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
                println!("[←] FEEFILTER — peer wants min fee of {} sat/kB", fee);
            }
        }

        // ── sendheaders ───────────────────────────────────────────────────
        // Introduced in BIP 130. The peer is saying: "from now on, announce
        // new blocks by sending the 80-byte header directly instead of an inv.
        // This saves a round trip: inv → getdata → headers → getdata → block
        // becomes just: headers → getdata → block."
        // We acknowledge by logging it (we don't need to reply).
        "sendheaders" => {
            println!("[←] SENDHEADERS — peer prefers header-based block announcements");
        }

        // ── sendcmpct ─────────────────────────────────────────────────────
        // Compact block relay (BIP 152). When a peer mines or receives a new block,
        // instead of sending the full block, they send:
        //   1. A compact block: header + short transaction IDs
        //   2. We reconstruct the block from our mempool (we already have most txs)
        //   3. Only request the transactions we're missing
        // This dramatically reduces block propagation latency.
        "sendcmpct" => {
            println!("[←] SENDCMPCT — peer supports compact block relay (BIP 152)");
        }

        // ── getheaders ────────────────────────────────────────────────────
        // The peer is asking US to send them block headers starting from
        // a particular point in the chain. Since we have no blockchain data,
        // we simply ignore this request. A full node would respond with `headers`.
        "getheaders" => {
            println!("[←] GETHEADERS (ignoring — we have no headers to send)");
        }

        // ── getdata ───────────────────────────────────────────────────────
        // The peer is requesting specific data items from us (blocks or transactions).
        // Since we don't store blocks or maintain a mempool, we ignore this.
        // A full node would respond with the requested `block` or `tx` messages.
        "getdata" => {
            println!("[←] GETDATA (ignoring — we have no data to serve)");
        }

        // ── reject ────────────────────────────────────────────────────────
        // The peer is telling us it rejected something we sent.
        // Payload format: varstr(message_type) + byte(code) + varstr(reason)
        // This is very useful for debugging: if you send a malformed transaction
        // or version message, the reject tells you exactly why it was refused.
        "reject" => {
            println!("[←] REJECT — peer rejected something ({} bytes)", msg.payload.len());
        }

        // ── Unknown / unhandled message ───────────────────────────────────
        // Bitcoin's protocol is extensible. New message types are added over time
        // (e.g. `wtxidrelay` added in Bitcoin Core 0.21 for SegWit tx relay).
        // We log unknown messages rather than erroring — this lets us observe
        // what newer nodes send without crashing.
        other => {
            println!(
                "[←] UNKNOWN command='{}' payload={} bytes",
                other, msg.payload.len()
            );
        }
    }

    Ok(())
}

// Parse and log the contents of an `inv` message payload.
//
// inv messages are the backbone of Bitcoin's gossip network. Every time a node
// receives a new transaction or mines/receives a block, it tells all its peers
// via inv. This function decodes the list and prints each item.
//
// Payload format:
//   varint(count)                    — how many items follow
//   [count × (4-byte type + 32-byte hash)]   — the inventory items
fn parse_and_log_inv(payload: &[u8]) {
    // Read the count varint at the start of the payload
    let (count, mut offset) = match decode_varint(payload, 0) {
        Ok(r)  => r,
        Err(e) => { println!("[←] INV (could not parse count: {})", e); return; }
    };

    println!("[←] INV — {} item(s):", count);

    // Read each 36-byte inventory entry: 4-byte type + 32-byte hash
    for i in 0..count.min(10) { // Cap at 10 entries to avoid flooding the terminal
        if offset + 36 > payload.len() {
            println!("    [payload truncated]");
            break;
        }

        // Read the 4-byte inventory type (little-endian u32)
        let inv_type = u32::from_le_bytes(payload[offset..offset+4].try_into().unwrap());

        // Read the 32-byte hash (txid or block hash)
        let hash_slice = &payload[offset+4..offset+36];
        offset += 36; // Advance to the next entry

        // Reverse the hash bytes for display (Bitcoin's display convention)
        let mut display = [0u8; 32];
        display.copy_from_slice(hash_slice);
        display.reverse();

        // Map the type integer to a human-readable name
        let type_str = match inv_type {
            1 => "TX",             // Unconfirmed transaction
            2 => "BLOCK",          // Full block
            3 => "FILTERED_BLOCK", // Merkle block (for Bloom filter clients)
            4 => "CMPCT_BLOCK",    // Compact block (BIP 152)
            _ => "UNKNOWN",
        };

        println!("    [{:2}] {} {}", i, type_str, hex_encode(&display));
    }

    // If there were more than 10 items, note the remainder
    if count > 10 {
        println!("    ... and {} more item(s)", count - 10);
    }
}
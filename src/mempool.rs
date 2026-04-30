// mempool.rs — Mempool Snapshot: Request and Display All Pending Transactions
//
// ── What is the Mempool? ─────────────────────────────────────────────────────
//
// Every full Bitcoin node keeps a "mempool" (memory pool) — an in-RAM waiting
// room of validated but unconfirmed transactions. Think of it as the queue
// before the checkout (the blockchain).
//
// The lifecycle of a transaction:
//
//   1. Alice broadcasts her signed transaction to the network
//   2. Every node that receives it validates it (correct signatures, not double-spend)
//   3. Valid transactions are held in the mempool
//   4. Miners pick transactions from the mempool — usually highest fee rate first
//   5. The chosen transactions are bundled into a new block and mined
//   6. Once confirmed in a block, the transactions leave the mempool permanently
//
// Key facts about mempools:
//   - Every node's mempool is slightly different (nodes hear txs at different times)
//   - Mainnet mempools often hold thousands of transactions during busy periods
//   - Testnet mempools are much smaller — usually a few dozen to a few hundred txs
//   - Mempools are NOT stored on disk — if you restart a node, the mempool is empty
//
// ── The `mempool` P2P Message (BIP 35) ──────────────────────────────────────
//
// Bitcoin has a dedicated message for requesting a peer's mempool contents.
// It was added in BIP 35 (Bitcoin Improvement Proposal number 35).
//
// The `mempool` message has ZERO payload bytes. It's just a signal.
// Sending it is like asking: "Hey peer, what transactions are you currently holding?"
//
// The peer responds with one or more `inv` messages, each containing up to
// 50,000 txids. On testnet you'll typically get a single small inv response.
//
// ── Full Message Flow ────────────────────────────────────────────────────────
//
//   [After handshake completes]
//
//   YOU  ──► mempool        empty payload — "please share your mempool"
//              ↓
//   PEER ──► inv            list of txids currently in their mempool
//              ↓
//   YOU       [optional]    send getdata for each txid to fetch full tx data
//              ↓
//   PEER ──► tx, tx, tx...  one tx message per txid you requested
//
// ── How This Module Fits Into the Codebase ───────────────────────────────────
//
//   main.rs          — calls request_mempool_snapshot() after handshake
//   peer.rs (inv)    — calls handle_mempool_inv() when inv arrives post-mempool
//   transaction.rs   — called by this module to optionally fetch full tx data
//
// The tricky part: when we receive an `inv` in the message loop, we don't
// automatically know if it's a mempool response or a new-transaction announcement.
// We track this with a simple boolean flag `mempool_requested` in main.rs.

use std::net::TcpStream;
use crate::peer::send_message;
use crate::encoding::decode_varint;
use crate::crypto::hex_encode;
use crate::transaction::request_transaction;

// ── PART 1: Sending the Request ──────────────────────────────────────────────

// Send a `mempool` message to the connected peer.
//
// This is the simplest possible Bitcoin P2P message — no payload at all.
// Our send_message() function in peer.rs will still build the full 24-byte
// header (magic + command + length=0 + checksum of empty payload).
//
// WHEN TO CALL THIS:
//   Call this right after the handshake completes (after receiving `verack`).
//   In main.rs we call it when handshake_done first becomes true.
//
// WHAT HAPPENS NEXT:
//   Watch for an incoming `inv` message. The peer will send one (or more)
//   containing txids. Pass that inv payload to handle_mempool_inv() below.
//
// NOTE ON PEER SUPPORT:
//   Not all peers respond to `mempool`. Bitcoin Core disabled it on mainnet
//   by default in v0.21.0 to reduce DoS risk. However testnet nodes generally
//   still honour it. If no response comes within ~30 seconds, the keepalive
//   ping mechanism in main.rs will fire — that's normal, it just means the
//   peer didn't respond to mempool.
pub fn request_mempool_snapshot(stream: &mut TcpStream, magic: [u8; 4]) -> std::io::Result<()> {
    println!("\n[*] 📦 Sending `mempool` request to peer...");
    println!("    Peer will respond with an `inv` listing unconfirmed transactions.");
    println!("    (If no response arrives, the peer may not support this message)");

    // Empty payload — the message itself IS the request
    send_message(stream, "mempool", &[], magic)
}

// ── PART 2: Handling the Response ────────────────────────────────────────────

// Process an `inv` message that is a response to our mempool request.
//
// HOW THIS IS DIFFERENT FROM A REGULAR INV:
//   Regular `inv` messages arrive spontaneously when the peer learns about
//   new transactions or blocks. They typically contain 1-3 items.
//   A mempool response `inv` can contain thousands of txids all at once.
//   In main.rs we use the `mempool_requested` flag to tell them apart.
//
// PARAMETERS:
//   payload        — raw bytes of the `inv` message body (header stripped)
//   stream         — TCP connection, needed to optionally send getdata
//   magic          — network magic bytes
//   fetch_count    — how many transactions to actually fetch (to avoid flooding)
//                    pass 0 to just display the txids without fetching
//
// RETURNS:
//   A Vec of all txids found in the inv, in internal byte order.
//   Callers can use this list to request individual transactions later.
pub fn handle_mempool_inv(
    payload: &[u8],
    stream: &mut TcpStream,
    magic: [u8; 4],
    fetch_count: usize,
) -> Vec<[u8; 32]> {

    // ── Parse the inv payload ─────────────────────────────────────────────
    //
    // inv payload layout:
    //   [varint]       count — total number of inventory items
    //   [count × 36]   items:
    //       [4 bytes]    type  — 1=TX, 2=BLOCK, 3=FILTERED_BLOCK, 4=CMPCT_BLOCK
    //       [32 bytes]   hash  — the txid or block hash
    let (total_count, mut offset) = match decode_varint(payload, 0) {
        Ok(r) => r,
        Err(e) => {
            println!("[!] Could not parse mempool inv payload: {}", e);
            return Vec::new();
        }
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║              📦  MEMPOOL SNAPSHOT                           ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Unconfirmed transactions in peer's mempool: {:>5}          ║", total_count);
    println!("╠══════════════════════════════════════════════════════════════╣");

    let mut all_txids: Vec<[u8; 32]> = Vec::new();

    // How many txids to print to the terminal (avoid flooding)
    let display_limit = 10usize;
    let mut displayed = 0usize;

    // Iterate through every item in the inv payload
    for i in 0..total_count {
        // Safety check — make sure we have 36 bytes left
        if offset + 36 > payload.len() {
            println!("║  [!] Payload ended early at item {} of {}", i, total_count);
            break;
        }

        // Read the 4-byte type field (little-endian u32)
        let inv_type = u32::from_le_bytes(
            payload[offset..offset + 4].try_into().unwrap()
        );
        offset += 4;

        // Read the 32-byte hash
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;

        // We only care about transactions (type 1). A mempool response should
        // be all TXs, but we filter defensively.
        if inv_type == 1 {
            all_txids.push(hash);

            // Print up to display_limit txids, then show a "and X more" message
            if displayed < display_limit {
                let mut display_hash = hash;
                display_hash.reverse(); // reverse for human-readable display
                println!("║  [{:>4}]  TX  {}", all_txids.len(), hex_encode(&display_hash));
                displayed += 1;

                if displayed == display_limit && total_count as usize > display_limit {
                    println!("║  ... and {} more (only showing first {})",
                        total_count as usize - display_limit,
                        display_limit
                    );
                }
            }
        }
    }

    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Total TX found: {}{}",
        all_txids.len(),
        " ".repeat(46usize.saturating_sub(all_txids.len().to_string().len()))
    );

    // ── Optionally fetch full transaction data ────────────────────────────
    //
    // We now have a list of txids from the mempool. We can request the full
    // transaction data for some of them using getdata (from transaction.rs).
    //
    // We limit this to `fetch_count` to avoid requesting thousands of
    // transactions at once and overwhelming our read buffer.
    let to_fetch = fetch_count.min(all_txids.len());
    if to_fetch > 0 {
        println!("║  Fetching full data for the first {} transaction(s)...", to_fetch);
        println!("╚══════════════════════════════════════════════════════════════╝");

        for txid in all_txids.iter().take(to_fetch) {
            // request_transaction (from transaction.rs) sends a `getdata` message
            // for this txid. The peer will respond with a `tx` message which
            // the message loop in main.rs will handle by calling
            // transaction::decode_and_display_tx().
            if let Err(e) = request_transaction(stream, txid, magic) {
                println!("[!] Failed to send getdata for tx: {}", e);
            }
        }
    } else {
        println!("╚══════════════════════════════════════════════════════════════╝");
    }

    all_txids
}

// ── PART 3: Summary Statistics ────────────────────────────────────────────────

// Print a summary of a mempool snapshot result.
//
// Called after handle_mempool_inv() to give a final count and status line.
// Separated out so main.rs can call it at the right point in the flow.
pub fn print_mempool_summary(txids: &[[u8; 32]]) {
    println!("\n[*] 📦 Mempool snapshot complete.");
    println!("    Total unconfirmed transactions seen: {}", txids.len());
    if txids.is_empty() {
        println!("    (The peer may not support `mempool` or its mempool is empty)");
    } else {
        println!("    Tip: these transactions are waiting to be mined into a block.");
        println!("    You can look any of them up on: https://mempool.space/testnet");
    }
}

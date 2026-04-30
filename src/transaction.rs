// transaction.rs — Transaction Fetching and Decoding
//
// ── What This File Does ───────────────────────────────────────────────────────
//
// This module handles two things:
//
//   1. REQUESTING a transaction from the peer
//      When the peer sends an `inv` message saying "I have TX abc123...",
//      we send back a `getdata` message saying "ok, send me the full data".
//      The peer then sends us a `tx` message with the raw transaction bytes.
//
//   2. DECODING that raw transaction
//      Bitcoin transactions are pure binary. We parse those bytes field by
//      field and print a readable breakdown: inputs, outputs, amounts, scripts.
//
// ── How Bitcoin Transactions Actually Work ────────────────────────────────────
//
// Think of Bitcoin like a chain of envelopes:
//
//   - Every output is a "locked envelope" containing some BTC.
//   - Every input "opens" a previous envelope (proves you own it) and spends it.
//   - The transaction creates NEW locked envelopes for the recipients.
//
// Example: Alice pays Bob 0.5 BTC
//
//   INPUT:  "I'm spending output #0 from tx a1b2c3... (Alice's envelope)"
//           scriptSig: [Alice's signature + public key — proves she owns it]
//
//   OUTPUT 0: 0.5 BTC → Bob's locking script
//   OUTPUT 1: 0.0498 BTC → Alice's change address  (she pays ~0.0002 BTC fee)
//
//   The miner fee is implicit: input_total - output_total = fee (never stated explicitly)
//
// ── Wire Format: Legacy Transaction ──────────────────────────────────────────
//
//   [4 bytes]  version       — 1 or 2 (version 2 enables relative timelocks)
//   [varint]   input_count
//   [inputs]   each input:
//                [32 bytes] prev_txid   — which previous transaction
//                [4 bytes]  prev_index  — which output of that tx (0-indexed)
//                [varint]   script_len
//                [N bytes]  scriptSig   — the "unlocking" script (signature, pubkey)
//                [4 bytes]  sequence
//   [varint]   output_count
//   [outputs]  each output:
//                [8 bytes]  value       — amount in satoshis (u64 little-endian)
//                [varint]   script_len
//                [N bytes]  scriptPubKey — the "locking" script (who can spend this)
//   [4 bytes]  locktime
//
// ── Wire Format: SegWit Transaction (BIP 141) ────────────────────────────────
//
// SegWit (Segregated Witness) was activated in 2017. It moved signature data
// (the "witness") out of the main transaction body to fix transaction malleability
// and reduce the effective size of transactions for fee purposes.
//
// SegWit transactions add two marker bytes after version:
//   [4 bytes]  version
//   [1 byte]   marker = 0x00  ← this signals "I am a SegWit transaction"
//   [1 byte]   flag   = 0x01  ← must be non-zero
//   [inputs]   (same as legacy, but scriptSig is often empty for SegWit inputs)
//   [outputs]  (same as legacy)
//   [witness]  one stack per input — contains the signatures moved out of scriptSig
//   [4 bytes]  locktime
//
// The marker 0x00 is safe to use because input_count can never be 0 in a
// valid legacy transaction — so seeing 0x00 where input_count should be
// is an unambiguous signal that this is SegWit.

use std::net::TcpStream;
use crate::peer::send_message;
use crate::encoding::decode_varint;
use crate::crypto::hex_encode;

// ── PART 1: Requesting a Transaction ─────────────────────────────────────────

// Build and send a `getdata` message to request a specific transaction.
//
// HOW GETDATA WORKS:
//   getdata is the Bitcoin protocol's general "please send me this data" message.
//   It's used for requesting transactions, blocks, and compact blocks.
//   The payload is identical in structure to an `inv` payload:
//
//     [varint]       count — how many items we want
//     [count × 36]   items — each is: [4-byte type] + [32-byte hash]
//
//   Item type values:
//     1 = MSG_TX             — want a full transaction
//     2 = MSG_BLOCK          — want a full block
//     3 = MSG_FILTERED_BLOCK — want a merkle block (SPV wallets use this)
//     4 = MSG_CMPCT_BLOCK    — want a compact block (BIP 152)
//
// PARAMETERS:
//   stream — the active TCP connection to our peer
//   txid   — the 32-byte transaction ID in internal byte order
//             (the same byte order as received in the inv message — do NOT reverse)
//   magic  — network identifier (MAGIC_TESTNET in our case)
//
// WHAT HAPPENS AFTER:
//   The peer receives this getdata, looks up the transaction in its mempool
//   or transaction index, and sends back a `tx` message. That `tx` message
//   is caught by the message loop in main.rs and routed to decode_and_display_tx().
pub fn request_transaction(
    stream: &mut TcpStream,
    txid: &[u8; 32],
    magic: [u8; 4],
) -> std::io::Result<()> {
    let mut payload = Vec::new();

    // Item count: we're requesting exactly 1 item.
    // varint values 0-252 are encoded as a single byte, so 0x01 = "count of 1".
    payload.push(0x01);

    // Item type: MSG_TX = 1, encoded as a 4-byte little-endian u32.
    // Little-endian means the least-significant byte comes first:
    //   1u32 in LE = [0x01, 0x00, 0x00, 0x00]
    let msg_tx: u32 = 1;
    payload.extend_from_slice(&msg_tx.to_le_bytes());

    // Item hash: the 32-byte txid, exactly as received from the inv message.
    // We do NOT reverse it here. The inv gave it to us in the internal byte
    // order that the peer uses — we pass it straight back the same way.
    payload.extend_from_slice(txid);

    // Display the txid reversed (Bitcoin's display convention) so it matches
    // what you'd see on a block explorer website.
    let mut display_txid = *txid;
    display_txid.reverse();
    println!("    [→] Requesting full TX data for: {}", hex_encode(&display_txid));

    // send_message (from peer.rs) wraps this payload in a 24-byte header
    // (magic + "getdata" + payload_length + checksum) and writes it to the socket.
    send_message(stream, "getdata", &payload, magic)
}

// ── PART 2: Data Structures for a Parsed Transaction ─────────────────────────

// Represents one input in a Bitcoin transaction.
//
// An input "opens" and spends a previous unspent output (UTXO).
// It must prove ownership of that output via the scriptSig (or witness for SegWit).
pub struct TxInput {
    // The txid of the transaction that created the output we're spending.
    // In internal byte order (reversed from what block explorers show).
    pub prev_txid: [u8; 32],

    // Which output of prev_txid we're spending (0-indexed).
    // If prev_txid is all zeros and this is 0xFFFFFFFF, it's a COINBASE input
    // (the special input that claims the block reward — no real prev output).
    pub prev_index: u32,

    // The "unlocking script" — contains the signature and public key that
    // prove we're allowed to spend this output.
    // In SegWit inputs, this is often empty (the proof is in the witness instead).
    pub script_sig: Vec<u8>,

    // Used for Replace-By-Fee (RBF) and relative timelocks.
    // 0xFFFFFFFF = default (no special meaning, final).
    // Any other value signals the tx can be replaced or has a relative timelock.
    pub sequence: u32,
}

// Represents one output in a Bitcoin transaction.
//
// An output creates a new "coin slot" — some amount of BTC locked by a script.
// This output becomes a UTXO (Unspent Transaction Output) that can be spent
// in a future transaction by whoever can satisfy the locking script.
pub struct TxOutput {
    // Amount in satoshis. 1 BTC = 100,000,000 satoshis.
    // Stored as u64 because the maximum supply (21 million BTC) fits in u64.
    pub value: u64,

    // The "locking script" — defines WHO can spend this output and HOW.
    // Common types: P2PKH (pay to public key hash), P2SH, P2WPKH, P2WSH.
    pub script_pubkey: Vec<u8>,
}

// Represents a fully parsed Bitcoin transaction.
pub struct Transaction {
    pub version: i32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub locktime: u32,
    pub is_segwit: bool, // True if this transaction uses the SegWit format
}

// ── PART 3: Parsing Raw Transaction Bytes ────────────────────────────────────

// Parse a raw `tx` message payload into a Transaction struct.
//
// `data` is the bytes we received from the peer's `tx` message (payload only,
// the 24-byte message header has already been stripped by the parser).
//
// We track our position through the byte slice with `offset`, advancing it
// as we read each field. This is a common pattern for binary protocol parsing.
pub fn parse_transaction(data: &[u8]) -> Result<Transaction, String> {
    let mut offset = 0usize;

    // ── Field: version (4 bytes, signed int, little-endian) ──────────────
    if data.len() < offset + 4 {
        return Err("TX data too short for version field".to_string());
    }
    // try_into() converts &[u8] slice into [u8; 4] — safe because we checked length.
    let version = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    offset += 4;

    // ── Detect SegWit (check for 0x00 marker byte) ───────────────────────
    //
    // In a legacy tx, the next byte would be the input_count varint, which
    // can NEVER be 0x00 (a tx with zero inputs is invalid). So if we see
    // 0x00 here, it's the SegWit marker. After the marker comes the flag byte
    // which must be 0x01.
    let is_segwit = data.len() > offset + 1 && data[offset] == 0x00;
    if is_segwit {
        offset += 1; // skip SegWit marker (0x00)
        if data[offset] != 0x01 {
            return Err("Invalid SegWit flag — expected 0x01".to_string());
        }
        offset += 1; // skip SegWit flag (0x01)
    }

    // ── Field: input_count (varint) + inputs ─────────────────────────────
    //
    // decode_varint returns (value, bytes_consumed).
    // We advance offset by bytes_consumed so we're positioned after the varint.
    let (input_count, consumed) = decode_varint(data, offset)?;
    offset += consumed;

    let mut inputs = Vec::new();
    for _ in 0..input_count {
        // prev_txid: 32 bytes
        if data.len() < offset + 32 {
            return Err("TX truncated while reading input prev_txid".to_string());
        }
        let mut prev_txid = [0u8; 32];
        prev_txid.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        // prev_index: 4 bytes, little-endian u32
        if data.len() < offset + 4 {
            return Err("TX truncated while reading input prev_index".to_string());
        }
        let prev_index = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        // scriptSig: varint(length) followed by `length` bytes
        let (script_len, consumed) = decode_varint(data, offset)?;
        offset += consumed;
        let script_len = script_len as usize;
        if data.len() < offset + script_len {
            return Err("TX truncated while reading input scriptSig".to_string());
        }
        let script_sig = data[offset..offset + script_len].to_vec();
        offset += script_len;

        // sequence: 4 bytes, little-endian u32
        if data.len() < offset + 4 {
            return Err("TX truncated while reading input sequence".to_string());
        }
        let sequence = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        inputs.push(TxInput { prev_txid, prev_index, script_sig, sequence });
    }

    // ── Field: output_count (varint) + outputs ────────────────────────────
    let (output_count, consumed) = decode_varint(data, offset)?;
    offset += consumed;

    let mut outputs = Vec::new();
    for _ in 0..output_count {
        // value: 8 bytes, little-endian u64 (satoshis)
        if data.len() < offset + 8 {
            return Err("TX truncated while reading output value".to_string());
        }
        let value = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // scriptPubKey: varint(length) followed by `length` bytes
        let (script_len, consumed) = decode_varint(data, offset)?;
        offset += consumed;
        let script_len = script_len as usize;
        if data.len() < offset + script_len {
            return Err("TX truncated while reading output scriptPubKey".to_string());
        }
        let script_pubkey = data[offset..offset + script_len].to_vec();
        offset += script_len;

        outputs.push(TxOutput { value, script_pubkey });
    }

    // ── Field: witness data (SegWit only) ────────────────────────────────
    //
    // In SegWit transactions there is one "witness stack" per input.
    // Each stack is a list of byte arrays (the signatures and pubkeys that
    // were moved out of scriptSig). We skip this data — it doesn't affect
    // the transaction's inputs/outputs, just the signature verification.
    if is_segwit {
        for _ in 0..inputs.len() {
            let (item_count, consumed) = decode_varint(data, offset)?;
            offset += consumed;
            for _ in 0..item_count {
                let (item_len, consumed) = decode_varint(data, offset)?;
                offset += consumed;
                offset += item_len as usize; // skip the witness bytes
            }
        }
    }

    // ── Field: locktime (4 bytes, little-endian u32) ─────────────────────
    //
    // Locktime restricts when this transaction can be mined:
    //   0            → no restriction (most transactions)
    //   < 500000000  → interpreted as a minimum block HEIGHT
    //   ≥ 500000000  → interpreted as a minimum Unix timestamp
    if data.len() < offset + 4 {
        return Err("TX truncated while reading locktime".to_string());
    }
    let locktime = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());

    Ok(Transaction { version, inputs, outputs, locktime, is_segwit })
}

// ── PART 4: Script Classification ────────────────────────────────────────────

// Identify a scriptPubKey's type and return a human-readable description.
//
// Bitcoin uses "scripts" — a simple stack-based language — to define spending
// conditions. There are standard patterns that wallets produce. Recognising
// these patterns lets us say "P2PKH" instead of printing raw hex.
//
// All byte values below are Bitcoin Script opcodes:
//   0x76 = OP_DUP          (duplicate top stack item)
//   0xa9 = OP_HASH160      (RIPEMD160(SHA256(top)))
//   0x14 = push 20 bytes
//   0x88 = OP_EQUALVERIFY  (check equal, fail if not)
//   0xac = OP_CHECKSIG     (verify signature against pubkey)
//   0x87 = OP_EQUAL
//   0x00 = OP_0 / OP_FALSE
//   0x20 = push 32 bytes
//   0x6a = OP_RETURN       (immediately fail — provably unspendable)
fn classify_script(script: &[u8]) -> String {
    match script {
        // P2PKH — Pay to Public Key Hash
        // The most traditional Bitcoin address type (starts with 'm' or 'n' on testnet)
        // Pattern: OP_DUP OP_HASH160 <20-byte hash> OP_EQUALVERIFY OP_CHECKSIG
        s if s.len() == 25
            && s[0] == 0x76  // OP_DUP
            && s[1] == 0xa9  // OP_HASH160
            && s[2] == 0x14  // push 20 bytes
            && s[23] == 0x88 // OP_EQUALVERIFY
            && s[24] == 0xac // OP_CHECKSIG
        => format!("P2PKH  hash160={}", hex_encode(&s[3..23])),

        // P2SH — Pay to Script Hash
        // Used for multisig and other complex scripts (starts with '2' on testnet)
        // Pattern: OP_HASH160 <20-byte hash> OP_EQUAL
        s if s.len() == 23
            && s[0] == 0xa9  // OP_HASH160
            && s[1] == 0x14  // push 20 bytes
            && s[22] == 0x87 // OP_EQUAL
        => format!("P2SH   hash160={}", hex_encode(&s[2..22])),

        // P2WPKH — Pay to Witness Public Key Hash (native SegWit, bech32 addresses)
        // Pattern: OP_0 <20-byte hash>
        s if s.len() == 22 && s[0] == 0x00 && s[1] == 0x14
        => format!("P2WPKH hash160={}", hex_encode(&s[2..])),

        // P2WSH — Pay to Witness Script Hash (SegWit multisig)
        // Pattern: OP_0 <32-byte hash>
        s if s.len() == 34 && s[0] == 0x00 && s[1] == 0x20
        => format!("P2WSH  hash256={}", hex_encode(&s[2..])),

        // OP_RETURN — Data embedding (provably unspendable)
        // Used to embed arbitrary data in the blockchain (e.g. for timestamping)
        s if !s.is_empty() && s[0] == 0x6a
        => format!("OP_RETURN data={}", hex_encode(&s[1..])),

        // Anything not matching a known pattern
        s => format!("UNKNOWN script={}", hex_encode(s)),
    }
}

// ── PART 5: Display a Decoded Transaction ────────────────────────────────────

// Decode and pretty-print a raw `tx` message payload.
//
// `payload`:   the raw bytes from the peer's `tx` message (header already stripped)
// `raw_txid`:  the txid in internal byte order (as received from the inv message)
//
// HOW THIS IS CALLED:
//   In main.rs, the message loop receives messages from the peer.
//   When it receives a `tx` message, it calls this function.
//   The `raw_txid` comes from the `inv` handler — we store the txid when we
//   request it, then pass it here when the peer responds.
pub fn decode_and_display_tx(payload: &[u8], raw_txid: &[u8; 32]) {
    // Reverse the txid for display — Bitcoin shows hashes in reversed byte order.
    // The internal storage order and the display order are opposite by convention.
    let mut display_txid = *raw_txid;
    display_txid.reverse();

    println!("\n    ╔══ TRANSACTION ══════════════════════════════════════════════");
    println!("    ║  TXID: {}", hex_encode(&display_txid));

    match parse_transaction(payload) {
        Err(e) => {
            println!("    ║  [!] Parse error: {}", e);
        }
        Ok(tx) => {
            println!("    ║  Version:  {}{}",
                tx.version,
                if tx.is_segwit { " (SegWit — witness data present)" } else { "" }
            );

            println!("    ║");
            println!("    ║  INPUTS: {}", tx.inputs.len());
            for (i, input) in tx.inputs.iter().enumerate() {
                // Reverse the prev_txid for display
                let mut display_prev = input.prev_txid;
                display_prev.reverse();

                // Coinbase inputs are special — they have no real previous output.
                // They're how miners claim the block reward + fees.
                let is_coinbase = input.prev_txid == [0u8; 32]
                    && input.prev_index == 0xFFFF_FFFF;

                if is_coinbase {
                    println!("    ║    [{}] COINBASE (block reward claim — no previous output)", i);
                } else {
                    println!("    ║    [{}] Spends output #{} of tx {}",
                        i, input.prev_index, hex_encode(&display_prev));
                }
                // Note: we deliberately don't print scriptSig bytes here to keep
                // output readable. In SegWit inputs scriptSig is often empty anyway.
            }

            println!("    ║");
            println!("    ║  OUTPUTS: {}", tx.outputs.len());
            let mut total_sats: u64 = 0;
            for (i, output) in tx.outputs.iter().enumerate() {
                total_sats += output.value;
                // Convert satoshis → BTC for display (8 decimal places)
                let btc = output.value as f64 / 100_000_000.0;
                let script_type = classify_script(&output.script_pubkey);
                println!("    ║    [{}] {:>14.8} BTC  →  {}", i, btc, script_type);
            }

            println!("    ║");
            println!("    ║  Total output: {:.8} BTC", total_sats as f64 / 100_000_000.0);
            println!("    ║  Locktime:     {}", match tx.locktime {
                0 => "0 (no restriction)".to_string(),
                n if n < 500_000_000 => format!("block height {}", n),
                n => format!("unix timestamp {}", n),
            });
        }
    }
    println!("    ╚═════════════════════════════════════════════════════════════");
}

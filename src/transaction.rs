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

use crate::encoding::decode_varint;

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
        offset += script_len; // Skip script_sig

        // sequence: 4 bytes, little-endian u32
        if data.len() < offset + 4 {
            return Err("TX truncated while reading input sequence".to_string());
        }
        offset += 4; // Skip sequence

        inputs.push(TxInput { prev_txid, prev_index });
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


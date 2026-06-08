// src/transaction.rs — Bitcoin transaction structures and parsing

// Legacy Transaction Wire Format:
//   [4 bytes]  version
//   [varint]   input_count
//   [inputs]   each input:
//                [32 bytes] prev_txid
//                [4 bytes]  prev_index
//                [varint]   script_len
//                [N bytes]  scriptSig
//                [4 bytes]  sequence
//   [varint]   output_count
//   [outputs]  each output:
//                [8 bytes]  value (satoshis)
//                [varint]   script_len
//                [N bytes]  scriptPubKey
//   [4 bytes]  locktime
//
// SegWit Transaction Wire Format (BIP 141):
//   [4 bytes]  version
//   [1 byte]   marker = 0x00
//   [1 byte]   flag   = 0x01
//   [inputs]   (same as legacy, scriptSig often empty)
//   [outputs]  (same as legacy)
//   [witness]  one stack per input (signatures and keys)
//   [4 bytes]  locktime

use crate::encoding::decode_varint;

pub struct TxInput {
    pub prev_txid: [u8; 32],
    pub prev_index: u32,
}

pub struct TxOutput {
    pub value: u64, // value in satoshis
    pub script_pubkey: Vec<u8>,
}

pub struct Transaction {
    pub version: i32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub locktime: u32,
    pub is_segwit: bool,
}

// Parses a raw `tx` message payload into a Transaction struct.
pub fn parse_transaction(data: &[u8]) -> Result<Transaction, String> {
    let mut offset = 0usize;

    if data.len() < offset + 4 {
        return Err("TX data too short for version field".to_string());
    }
    let version = i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    offset += 4;

    // Detect SegWit transaction (BIP 141 marker and flag)
    let is_segwit = data.len() > offset + 1 && data[offset] == 0x00;
    if is_segwit {
        offset += 1; // skip marker
        if data[offset] != 0x01 {
            return Err("Invalid SegWit flag — expected 0x01".to_string());
        }
        offset += 1; // skip flag
    }

    let (input_count, consumed) = decode_varint(data, offset)?;
    offset += consumed;

    let mut inputs = Vec::new();
    for _ in 0..input_count {
        if data.len() < offset + 32 {
            return Err("TX truncated while reading input prev_txid".to_string());
        }
        let mut prev_txid = [0u8; 32];
        prev_txid.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        if data.len() < offset + 4 {
            return Err("TX truncated while reading input prev_index".to_string());
        }
        let prev_index = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        let (script_len, consumed) = decode_varint(data, offset)?;
        offset += consumed;
        let script_len = script_len as usize;
        if data.len() < offset + script_len {
            return Err("TX truncated while reading input scriptSig".to_string());
        }
        offset += script_len; // skip script_sig

        if data.len() < offset + 4 {
            return Err("TX truncated while reading input sequence".to_string());
        }
        offset += 4; // skip sequence

        inputs.push(TxInput { prev_txid, prev_index });
    }

    let (output_count, consumed) = decode_varint(data, offset)?;
    offset += consumed;

    let mut outputs = Vec::new();
    for _ in 0..output_count {
        if data.len() < offset + 8 {
            return Err("TX truncated while reading output value".to_string());
        }
        let value = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());
        offset += 8;

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

    // Skip witness stack if SegWit
    if is_segwit {
        for _ in 0..inputs.len() {
            let (item_count, consumed) = decode_varint(data, offset)?;
            offset += consumed;
            for _ in 0..item_count {
                let (item_len, consumed) = decode_varint(data, offset)?;
                offset += consumed;
                offset += item_len as usize; // skip witness bytes
            }
        }
    }

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


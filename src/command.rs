// src/command.rs — Tauri command handlers

use tauri::{AppHandle, State};
use std::sync::Arc;
use crate::node::{SharedState, run_bitcoin_node};
use crate::message::MAGIC_TESTNET;
use crate::peer::send_message;

#[tauri::command]
pub fn connect(
    app_handle: AppHandle,
    state: State<SharedState>,
) -> Result<String, String> {
    let mut node = state.lock().map_err(|e| e.to_string())?;

    if node.should_run {
        return Err("Already connected or connecting".to_string());
    }

    node.should_run = true;
    drop(node);

    let state_clone = Arc::clone(&state);
    let handle_clone = app_handle.clone();

    std::thread::spawn(move || {
        run_bitcoin_node(handle_clone, state_clone);
    });

    Ok("Connecting...".to_string())
}

// ── Command 2: disconnect ─────────────────────────────────────────────────────
//
// Called from JS when the user clicks "Disconnect".
// Sets should_run = false, which the background thread checks on each iteration.
//
// JS usage:
//   await invoke("disconnect")
#[tauri::command]
pub fn disconnect(state: State<SharedState>) -> Result<String, String> {
    let mut node = state.lock().map_err(|e| e.to_string())?;
    node.should_run = false;
    Ok("Disconnecting...".to_string())
}

#[tauri::command]
pub fn request_tx(
    txid_hex: String,
    state: State<SharedState>,
) -> Result<String, String> {
    if txid_hex.len() != 64 {
        return Err("TXID must be exactly 64 hex characters".to_string());
    }

    let mut bytes = [0u8; 32];
    for i in 0..32 {
        let byte_str = &txid_hex[i * 2..i * 2 + 2];
        bytes[i] = u8::from_str_radix(byte_str, 16)
            .map_err(|_| "Invalid hex character in TXID".to_string())?;
    }
    bytes.reverse(); // Display order to internal byte order

    let mut node = state.lock().map_err(|e| e.to_string())?;
    if !node.is_connected {
        return Err("Not connected to a peer".to_string());
    }

    let stream = node.stream.as_mut().ok_or("No active stream")?;

    // getdata: count (varint) + type (4 bytes) + hash (32 bytes)
    // Using MSG_WITNESS_TX (0x40000001) for SegWit support
    let mut payload = Vec::new();
    payload.push(0x01);
    payload.extend_from_slice(&0x40000001u32.to_le_bytes());
    payload.extend_from_slice(&bytes);

    send_message(stream, "getdata", &payload, MAGIC_TESTNET)
        .map_err(|e| e.to_string())?;

    Ok(format!("Requested TX: {}...", &txid_hex[..16]))
}

#[tauri::command]
pub fn request_mempool(state: State<SharedState>) -> Result<String, String> {
    let mut node = state.lock().map_err(|e| e.to_string())?;
    if !node.is_connected {
        return Err("Not connected to a peer".to_string());
    }

    let stream = node.stream.as_mut().ok_or("No active stream")?;

    send_message(stream, "mempool", &[], MAGIC_TESTNET)
        .map_err(|e| e.to_string())?;

    node.mempool_requested = true;
    node.mempool_done = false;

    Ok("Mempool request sent".to_string())
}
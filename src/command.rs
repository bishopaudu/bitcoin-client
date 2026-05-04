// src/commands.rs — Tauri command handlers
//
// Functions in this file are callable from JavaScript via:
//   await invoke("function_name", { arg: value })
//
// Rules for Tauri commands:
//   1. Must be marked with #[tauri::command]
//   2. Must return a type that implements Serialize (so it can become JSON)
//   3. Can take a State<T> parameter to access shared state
//   4. Can be async (we use sync here for simplicity)
//
// The `State<AppState>` parameter is how commands access the shared NodeState.
// Tauri injects it automatically — you don't pass it from JS.

use tauri::{AppHandle, State};
use std::sync::Arc;
use crate::node::{SharedState, run_bitcoin_node};
use crate::message::MAGIC_TESTNET;
use crate::peer::send_message;

// ── Command 1: connect ────────────────────────────────────────────────────────
//
// Called from JS when the user clicks the "Connect" button.
// Spawns the background Bitcoin P2P thread.
//
// JS usage:
//   await invoke("connect")
//
// What it does:
//   1. Sets state.should_run = true
//   2. Spawns a new OS thread running run_bitcoin_node()
//   3. Returns immediately — the thread runs in the background
//
// Why we return immediately:
//   Tauri commands run on a thread pool. If we blocked here waiting for
//   the Bitcoin connection, the command would hang and the UI would appear
//   frozen. We spawn a thread and return right away; the thread emits
//   events as things happen.
#[tauri::command]
pub fn connect(
    app_handle: AppHandle,
    state: State<SharedState>,
) -> Result<String, String> {
    // Lock the mutex to check/update state
    // `.lock()` blocks until no other thread holds the lock, then returns a guard
    // The guard auto-releases the lock when it goes out of scope
    let mut node = state.lock().map_err(|e| e.to_string())?;

    // Prevent double-connecting
    if node.should_run {
        return Err("Already connected or connecting".to_string());
    }

    // Signal the thread to run
    node.should_run = true;

    // We need to drop the lock BEFORE spawning the thread.
    // If we held the lock and the thread tried to acquire it, we'd deadlock.
    drop(node);

    // Clone state and app_handle for the new thread.
    // Arc::clone() doesn't copy the data — it just increments the reference count.
    // Both the main thread and the new thread now point to the SAME NodeState.
    let state_clone = Arc::clone(&state);
    let handle_clone = app_handle.clone();

    // std::thread::spawn creates a new OS thread.
    // The `move` keyword moves ownership of state_clone and handle_clone
    // into the closure — they'll live as long as the thread does.
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
//
// The thread exits cleanly on the next loop iteration rather than being
// forcefully killed. This ensures the TCP connection is closed properly.
#[tauri::command]
pub fn disconnect(state: State<SharedState>) -> Result<String, String> {
    let mut node = state.lock().map_err(|e| e.to_string())?;
    node.should_run = false;
    Ok("Disconnecting...".to_string())
}

// ── Command 3: request_tx ─────────────────────────────────────────────────────
//
// Called from JS when the user types a TXID and clicks "Look up".
// Sends a getdata message for that specific transaction.
//
// JS usage:
//   await invoke("request_tx", { txidHex: "a1b2c3...64 hex chars..." })
//
// Parameters:
//   txid_hex — the transaction ID in display order (64 hex chars, as shown
//               on block explorers). We reverse it to internal byte order
//               before sending to the peer.
#[tauri::command]
pub fn request_tx(
    txid_hex: String,
    state: State<SharedState>,
) -> Result<String, String> {
    // Validate input length
    if txid_hex.len() != 64 {
        return Err("TXID must be exactly 64 hex characters".to_string());
    }

    // Parse hex string into bytes
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        let byte_str = &txid_hex[i * 2..i * 2 + 2];
        bytes[i] = u8::from_str_radix(byte_str, 16)
            .map_err(|_| "Invalid hex character in TXID".to_string())?;
    }
    // Reverse: display order → internal byte order
    bytes.reverse();

    // Get the stream from shared state and send getdata
    let mut node = state.lock().map_err(|e| e.to_string())?;

    if !node.is_connected {
        return Err("Not connected to a peer".to_string());
    }

    let stream = node.stream.as_mut()
        .ok_or("No active stream")?;

    // Build getdata payload: varint(1) + type(4 bytes) + hash(32 bytes)
    let mut payload = Vec::new();
    payload.push(0x01);                               // count = 1
    // Use MSG_WITNESS_TX (0x40000001) instead of MSG_TX (1).
    // Modern nodes will silently ignore MSG_TX if the transaction is SegWit!
    payload.extend_from_slice(&0x40000001u32.to_le_bytes());   
    payload.extend_from_slice(&bytes);                // the txid

    send_message(stream, "getdata", &payload, MAGIC_TESTNET)
        .map_err(|e| e.to_string())?;

    Ok(format!("Requested TX: {}...", &txid_hex[..16]))
}

// ── Command 4: request_mempool ────────────────────────────────────────────────
//
// Called from JS when the user clicks "Fetch Mempool Snapshot".
// Sends the `mempool` message to the peer.
//
// JS usage:
//   await invoke("request_mempool")
//
// After this, the peer sends an `inv` with potentially thousands of txids.
// The background thread in node.rs detects it's a mempool response (via the
// mempool_requested flag) and emits a "mempool-snapshot" event to the UI.
#[tauri::command]
pub fn request_mempool(state: State<SharedState>) -> Result<String, String> {
    let mut node = state.lock().map_err(|e| e.to_string())?;

    if !node.is_connected {
        return Err("Not connected to a peer".to_string());
    }

    let stream = node.stream.as_mut()
        .ok_or("No active stream")?;

    send_message(stream, "mempool", &[], MAGIC_TESTNET)
        .map_err(|e| e.to_string())?;

    node.mempool_requested = true;
    node.mempool_done = false;

    Ok("Mempool request sent".to_string())
}
// src/main.rs — Tauri application entry point
//
// In a Tauri app, main() doesn't run your Bitcoin code directly.
// Instead it:
//   1. Creates the shared state (NodeState wrapped in Arc<Mutex>)
//   2. Registers your Rust commands so JS can call them
//   3. Hands control to Tauri, which opens the window and runs the event loop
//
// Your Bitcoin P2P code now lives in node.rs and is started by the
// "connect" command when the user clicks the Connect button in the UI.

// Suppress the default Windows console window that would appear behind the
// app window on Windows. Has no effect on Linux or macOS.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod encoding;
mod message;
mod version;
mod parser;
mod peer;
mod network;
mod transaction;
mod node;
mod command;

use std::sync::{Arc, Mutex};
use node::{NodeState, SharedState};

fn main() {
    // Create the initial shared state — not connected, not running
    let state: SharedState = Arc::new(Mutex::new(NodeState::new()));

    tauri::Builder::default()
        // Register our shared state with Tauri's dependency injection system.
        // After this, any command can receive it as State<SharedState>.
        .manage(state)

        // Register all callable commands.
        // The generate_handler! macro generates the routing code that maps
        // the string "connect" → commands::connect function, etc.
        // If you add a new command, add it to this list.
        .invoke_handler(tauri::generate_handler![
            command::connect,
            command::disconnect,
            command::request_tx,
            command::request_mempool,
        ])

        // Hand control to Tauri. This call never returns — it runs the
        // native OS event loop (processing window events, redraws, etc.)
        // until the user closes the window.
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
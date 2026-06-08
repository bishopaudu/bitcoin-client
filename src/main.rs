// src/main.rs — Tauri application entry point

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
    let state: SharedState = Arc::new(Mutex::new(NodeState::new()));

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            command::connect,
            command::disconnect,
            command::request_tx,
            command::request_mempool,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
# Project Migration: CLI to Tauri + React Desktop App

This document outlines the entire journey of migrating the original single-threaded Bitcoin command-line client into a modern, multi-threaded Desktop Application powered by Rust (Tauri) and React.

## 1. Architectural Overhaul (Rust Backend)
To prevent the UI from freezing while waiting for TCP network responses, the entire Bitcoin P2P loop was moved into a background thread.

* **`src/node.rs`**: Created to house the background thread logic (`run_bitcoin_node`). This thread handles the TCP stream, parses incoming Bitcoin messages, and emits asynchronous events to the frontend.
* **Shared State (`Arc<Mutex<NodeState>>`)**: Implemented thread-safe state management so the frontend commands can read/write the connection status without causing data races with the background thread.
* **Tauri Commands (`src/command.rs`)**: Built the IPC (Inter-Process Communication) bridge exposing Rust functions to Javascript:
  * `connect`: Spawns the background thread.
  * `disconnect`: Gracefully flags the thread to exit.
  * `request_tx`: Sends a `getdata` message to the peer.
  * `request_mempool`: Sends a `mempool` message to the peer.

## 2. Protocol & Networking Fixes
* **SegWit Compatibility**: Upgraded the `getdata` request in `request_tx` to ask for `MSG_WITNESS_TX` (`0x40000001`) instead of legacy `MSG_TX` (`1`). This prevents modern nodes from silently ignoring our transaction requests.
* **Error Handling (`notfound`)**: Updated the message parser to explicitly catch `notfound` messages from the peer, immediately forwarding a `transaction-notfound` event to the UI so the user isn't left waiting forever.

## 3. Frontend Migration (React + Vite)
We successfully transitioned from a raw Vanilla HTML/JS setup to an industry-standard React framework.

* **Scaffolding (`ui-react/`)**: Generated a blazing fast development environment using Vite, and installed the official `@tauri-apps/api` library to communicate with Rust.
* **Configuration (`tauri.conf.json`)**: Linked the Rust binary to the Vite development server (`http://localhost:5173`) to enable Hot-Module Replacement (HMR).
* **State Management (`App.jsx`)**: Replaced manual DOM mutations (`document.getElementById`) with React's `useState` and `useEffect` hooks.
  * Mapped Tauri events (`connection-status`, `peer-info`, `bitcoin-message`, etc.) directly into React state.
* **Cleanup**: Safely deleted the obsolete `ui/` directory.

## 4. UI / UX Enhancements
* **Live Message Log**: Built an auto-scrolling terminal window that renders incoming P2P messages in real-time. Added an "optimistic update" to inject outgoing `getdata` commands so the user gets instant visual feedback when fetching data.
* **Timeout Fallback**: Engineered a 5-second timeout in React. Because Bitcoin is a silent-failure protocol (peers often just ignore requests for transactions they don't have), the UI will auto-unlock the button and alert the user rather than freezing indefinitely.
* **Transaction Modal**: Refactored the transaction display into a beautiful, centered pop-up Modal. The modal explicitly displays the `fetched_from` IP address, confirming exactly which node provided the blockchain data.

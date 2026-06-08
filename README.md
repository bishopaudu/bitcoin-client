# Cthulhu: Bitcoin P2P Network Observer

Cthulhu is a modern desktop application designed to monitor the Bitcoin P2P protocol in real-time. Built with a Rust (Tauri) backend and a React frontend, it connects directly to live Bitcoin Testnet3 nodes to observe protocol traffic on the wire.

---

## Features

- **Protocol Handshake**: Connects to live Testnet3 nodes using DNS seed discovery and completes the version handshake (`version` ↔ `verack`).
- **Live P2P Message Log**: Streams real-time messages from the connected peer (e.g. `ping`, `feefilter`, `addr`, `inv`, `reject`, `notfound`) in a scrollable console.
- **Network Activity Feed**: Parses inventory announcements (`inv` vectors) to categorize and display live transaction and block propagation.
- **Peer Insights**: Displays protocol details of the connected peer, including software user agent, blockchain height, and services.
- **Transaction Lookup**: Queries details for any transaction ID (confirmed or unconfirmed) via the mempool.space API, showing a breakdown of inputs, outputs, locktime, and size.
- **Mempool Snapshot**: Fetches statistics (total fees, virtual size, count) and a preview list of unconfirmed transactions.

---

## Architecture

Cthulhu splits responsibilities between a high-performance Rust backend and a reactive TypeScript/JavaScript frontend:

- **Backend (Rust + Tauri)**:
  - Discovers peers via DNS seed resolution.
  - Manages raw TCP sockets and frames P2P messages using the 24-byte Bitcoin header format.
  - Runs network reading on a dedicated background OS thread to keep the interface non-blocking.
  - Emits structured event payloads to the frontend over Tauri's IPC bridge.
- **Frontend (React + Vite)**:
  - Displays connection state, live message streams, and network feeds.
  - Triggers connection commands (`connect`, `disconnect`).
  - Fetches transaction and mempool statistics via the mempool.space public REST API for reliability.

---

## Project Structure

```text
bitcoin-client/
├── Cargo.toml         - Rust dependencies and project metadata
├── tauri.conf.json    - Tauri app build and window configuration
├── src/               - Rust P2P backend
│   ├── main.rs        - Tauri builder and command registration
│   ├── command.rs     - IPC handlers invoked by React
│   ├── node.rs        - Background TCP stream worker and event dispatcher
│   ├── message.rs     - Bitcoin 24-byte header framing
│   ├── parser.rs      - Message stream parser and checksum validator
│   ├── version.rs     - Handshake payload builder and decoder
│   ├── encoding.rs    - Bitcoin wire formats (varint, varstr, net_addr)
│   ├── peer.rs        - TCP connection setup and stream writer
│   └── crypto.rs      - SHA-256 and hex formatting helpers
└── ui-react/          - React frontend
    ├── package.json   - Frontend NPM package manifest
    ├── index.html     - Entry document
    └── src/
        ├── App.jsx    - Main UI component and Tauri event listener
        └── components/ - Modular layout components and modals
```

---

## Getting Started

### Prerequisites

- **Rust**: Install via [rustup](https://rustup.rs/) (1.70+ recommended).
- **Node.js**: Install Node.js (v18+ recommended) and `npm`.

### Installation

1. Clone this repository.
2. Install Tauri CLI if you haven't already:
   ```bash
   cargo install tauri-cli
   ```
3. Install the frontend dependencies:
   ```bash
   cd ui-react
   npm install
   cd ..
   ```

### Running in Development

To start the application in development mode:
```bash
cargo tauri dev
```
This starts the Vite React dev server and launches the Tauri window with hot-reloading enabled for both frontend and backend.

### Building for Release

To compile a production-ready installer:
```bash
cargo tauri build
```

---

## License

MIT

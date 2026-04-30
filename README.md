# Bitcoin P2P Client (Rust)

A minimal, educational Bitcoin P2P network client built in Rust. 

This project was built to deeply understand how the Bitcoin network works at the protocol level: how nodes discover each other, how messages are framed in binary, how the version handshake works, and how to actively fetch and decode real-time transaction data directly from the wire.

---

## 🚀 Features

- **Peer Discovery**: Resolves Bitcoin testnet DNS seeds to find live peer IP addresses.
- **Protocol Handshake**: Performs the full Bitcoin version handshake (`version` → `verack`).
- **Live Network Monitor**: Enters a message loop to log network activity (`ping`, `feefilter`, `addr`, etc.).
- **Real-Time Transaction Fetcher**: Listens for transaction announcements (`inv`), actively requests the raw bytes (`getdata`), and decodes the binary payload into readable inputs/outputs.
- **Mempool Snapshots**: Can request a peer's entire queue of unconfirmed transactions via the `mempool` command.
- **Script Decoding**: Identifies and labels common Bitcoin locking scripts (e.g., P2PKH, P2SH, P2WPKH, P2WSH) from raw binary.

---

## 🛠️ Usage

**Requirements:** Rust 1.70+ and an internet connection (to reach testnet DNS seeds).

Clone the repository and run the client in one of three modes:

### 1. Passive Monitor (Default)
Connects to the network, completes the handshake, and listens. As the peer announces new live transactions, your client will automatically fetch and decode them in real-time.
```bash
cargo run
```

### 2. Fetch a Specific Transaction
Pass a 64-character transaction ID. The client will connect and immediately request that specific transaction from the peer's mempool.
```bash
cargo run -- <txid_hex>
# Example: cargo run -- c2f994087b93de0b883dd484f95060b19943108cac75d2aea0124f4cc3dfe682
```

### 3. Mempool Snapshot
Ask the peer to dump its unconfirmed transaction queue. *(Note: Some peers protect their CPU by disconnecting clients that send this command. If you get disconnected, simply run it again to try a different peer.)*
```bash
cargo run -- mempool
```

---

## 🔎 Example Output: Transaction Decoding

When the client fetches a transaction, it unpacks the raw binary into a readable format, identifying inputs, outputs, and SegWit data:

```text
    ╔══ TRANSACTION ══════════════════════════════════════════════
    ║  TXID: c2f994087b93de0b883dd484f95060b19943108cac75d2aea0124f4cc3dfe682
    ║  Version:  2 (SegWit — witness data present)
    ║
    ║  INPUTS: 1
    ║    [0] Spends output #0 of tx 01d1ee46abf721a4f2675702ed26d71058cf6afe4a71519a6121480723dc112e
    ║
    ║  OUTPUTS: 2
    ║    [0]    28.57135755 BTC  →  P2WPKH hash160=353b22d64a45ace75552bbecd3b1bb67fea3a942
    ║    [1]     0.00285190 BTC  →  P2WPKH hash160=96a544e5d8eab403d70d8b45b6a9a70fc7f91f68
    ║
    ║  Total output: 28.57420945 BTC
    ║  Locktime:     block height 4952888
    ╚═════════════════════════════════════════════════════════════
```

---

## 📁 Project Structure

```text
bitcoin-client/
├── Cargo.toml
└── src/
    ├── main.rs          — Entry point, CLI parsing, event loop orchestration
    ├── network.rs       — DNS seed resolution, testnet peer discovery
    ├── peer.rs          — TCP connection, timeout handling, message dispatcher
    ├── parser.rs        — Streaming message reader with checksum validation
    ├── message.rs       — MessageHeader struct, build_message, magic byte constants
    ├── version.rs       — Version handshake payload builder and decoder
    ├── encoding.rs      — Varint, varstr, net_addr encode/decode helpers
    ├── crypto.rs        — Hex encoding and SHA-256 wrappers
    ├── transaction.rs   — Requests and decodes raw binary transactions (inputs/outputs/scripts)
    └── mempool.rs       — Handles BIP 35 mempool snapshot requests and huge `inv` parsing
```

---

## 🧠 How Bitcoin Messaging Works

Every message on the Bitcoin P2P network — from a tiny `ping` to a 4 MB `block` — has the same 24-byte binary header:

```text
┌─────────────┬──────────────────┬─────────────┬──────────────┐
│ Magic       │ Command          │ Length      │ Checksum     │
│ 4 bytes     │ 12 bytes         │ 4 bytes LE  │ 4 bytes      │
├─────────────┴──────────────────┴─────────────┴──────────────┤
│ Payload (0 to N bytes)                                       │
└──────────────────────────────────────────────────────────────┘
```

- **Magic bytes** identify the network. Testnet is `0B 11 09 07`, mainnet is `F9 BE B4 D9`.
- **Command** is an ASCII string right-padded with null bytes to exactly 12 bytes.
- **Length** tells the parser how many payload bytes follow.
- **Checksum** is the first 4 bytes of `SHA256(SHA256(payload))` — used to detect corruption.

---

## 🤝 The Version Handshake

No data flows until both sides complete the handshake:

```text
Our client                         Testnet peer
    │                                   │
    │──── TCP SYN ──────────────────────►│
    │◄─── TCP SYN-ACK ──────────────────│
    │──── TCP ACK ──────────────────────►│  connected
    │                                   │
    │──── version ──────────────────────►│  "here is who I am"
    │◄─── version ──────────────────────│  "here is who I am"
    │◄─── verack ───────────────────────│  "I got yours"
    │──── verack ──────────────────────►│  "I got yours"
    │                                   │
    │        [handshake complete]       │
    │                                   │
    │◄─── sendheaders, feefilter, ping ─│
    │──── pong ────────────────────────►│
    │◄─── inv (new txs / blocks) ───────│
```

---

## 🌐 Network Configuration

Connects to **testnet3** by default (port 18333). To switch to mainnet, change the magic constant in `src/main.rs`:

```rust
// testnet (default)
let magic = message::MAGIC_TESTNET;

// mainnet
let magic = message::MAGIC_MAINNET;
```

And change the DNS seeds in `src/network.rs` to mainnet seeds (e.g., `seed.bitcoin.sipa.be`) and port `8333`.

---

## 📜 License

MIT


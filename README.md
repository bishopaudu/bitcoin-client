# bitcoin-p2p-client

A minimal Bitcoin P2P network client built from scratch in Rust — no Bitcoin libraries, no external dependencies, pure standard library only.

Built to deeply understand how the Bitcoin network works at the protocol level: how nodes discover each other, how messages are framed in binary, how the version handshake works, and what flows over the wire once two nodes are connected.

---

## What it does

- Resolves Bitcoin testnet DNS seeds to find a live peer
- Opens a raw TCP connection to that peer
- Performs the full Bitcoin version handshake (`version` → `verack`)
- Enters a message loop and logs every incoming message
- Responds to `ping` with `pong` to stay connected
- Requests peer addresses via `getaddr` after the handshake completes

---

## Project structure

```
bitcoin-p2p-client/
├── Cargo.toml
└── src/
    ├── main.rs        — entry point, event loop, module wiring
    ├── crypto.rs      — SHA256 and double-SHA256 from scratch, hex encoding
    ├── message.rs     — MessageHeader struct, build_message, magic byte constants
    ├── encoding.rs    — varint, varstr, net_addr encode/decode helpers
    ├── version.rs     — version payload builder and decoder
    ├── parser.rs      — streaming message reader with checksum validation
    ├── peer.rs        — TCP connection, send_message, message dispatcher
    └── network.rs     — DNS seed resolution, testnet peer discovery
```

Dependency flow (strictly one-way, nothing circular):

```
main
 ├── peer      ──► message, version, encoding, crypto
 ├── parser    ──► message, crypto
 └── network   (no protocol deps)
      message  ──► crypto
      version  ──► encoding
      encoding (no deps)
      crypto   (no deps)
```

---

## Getting started

**Requirements:** Rust 1.70+ and an internet connection (to reach testnet DNS seeds).

```bash
git clone https://github.com/yourname/bitcoin-p2p-client
cd bitcoin-p2p-client
cargo run
```

You should see output like:

```
╔══════════════════════════════════════════╗
║   Rust Bitcoin P2P Client — Testnet3     ║
╚══════════════════════════════════════════╝

[*] DNS seed 'testnet-seed.bitcoin.jonasschnelli.ch' → 203.0.113.45:18333
[*] Target peer: 203.0.113.45:18333
[+] TCP connection established to 203.0.113.45:18333
[→] Sending 'version' (86 bytes payload, 110 bytes total)

[MSG #0001] ─────────────────────────────────────
[←] VERSION: version=70015 services=0x0000000000000409 user_agent='/Satoshi:25.0.0/' height=2500341

[MSG #0002] ─────────────────────────────────────
[←] VERACK — handshake complete! Ready to receive network data.

[MSG #0003] ─────────────────────────────────────
[←] SENDHEADERS — peer prefers header-based block announcements

[MSG #0004] ─────────────────────────────────────
[←] FEEFILTER — peer wants min fee of 1000 sat/kB

[MSG #0005] ─────────────────────────────────────
[←] SENDCMPCT — peer supports compact block relay (BIP 152)
[*] Requesting peer addresses...

[MSG #0006] ─────────────────────────────────────
[←] PING nonce=A3F2109C44DE812B — responding with pong

[MSG #0007] ─────────────────────────────────────
[←] INV — 5 item(s):
    [ 0] TX a1b2c3d4e5f6...
    [ 1] TX 9f8e7d6c5b4a...
    ...
```

---

## How Bitcoin messaging works

Every message on the Bitcoin P2P network — from a tiny `ping` to a 4 MB `block` — has the same 24-byte binary header:

```
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

The client implements all of this manually, including the SHA256 algorithm itself.

---

## The version handshake

No data flows until both sides complete the handshake:

```
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
    │        [handshake complete]        │
    │                                   │
    │◄─── sendheaders, feefilter, ping ─│
    │──── pong ────────────────────────►│
    │◄─── inv (new txs / blocks) ───────│
```

The `version` payload includes: protocol version number, services bitmask, timestamp, peer addresses, a random nonce (for loop detection), user agent string, best block height, and a relay flag.

---

## Message types handled

| Message | Direction | What it means |
|---|---|---|
| `version` | ← → | Handshake introduction. Must be first. |
| `verack` | ← → | Acknowledges the other side's version. |
| `ping` | ← | Keepalive probe. We respond with `pong`. |
| `pong` | → | Our response to ping. Contains the same nonce. |
| `inv` | ← | Peer announces new transactions or blocks it has. |
| `addr` | ← | List of other peer addresses the node knows about. |
| `headers` | ← | Block headers (without transaction data). |
| `block` | ← | A full block. We log its hash. |
| `tx` | ← | A raw transaction. We log its txid. |
| `feefilter` | ← | Peer's minimum fee rate for transaction relay. |
| `sendheaders` | ← | Peer prefers block announcements via headers. |
| `sendcmpct` | ← | Peer supports compact block relay (BIP 152). |
| `getaddr` | → | We send this to request more peer addresses. |

---

## No external dependencies

`Cargo.toml` has an empty `[dependencies]` section. Everything is built on Rust's standard library:

- SHA256 — implemented from the FIPS 180-4 spec
- TCP — `std::net::TcpStream`
- DNS resolution — `std::net::ToSocketAddrs`
- Binary serialization — manual byte manipulation with `to_le_bytes()` / `from_le_bytes()`

---

## What you could build on top of this

This client is a foundation. The protocol primitives are all here — extending it is a matter of adding message handlers.

**Network crawler** — when you receive an `addr` message, connect to a few of those peers and ask them for their peers too. Within minutes you're mapping the entire reachable testnet topology.

**Transaction propagation tracker** — connect to 20–50 peers simultaneously. Record the timestamp when you first see each txid via `inv`. Track how long it takes to see the same txid from all other peers — this measures real gossip propagation latency.

**Mempool monitor** — instead of ignoring `inv` messages for transactions, send `getdata` back for every TX entry. You'll receive every unconfirmed transaction in real time. Parse fee rates and you have a live mempool fee estimator.

**SPV wallet** — send `getheaders` to download all block headers without full block data. Add BIP 157/158 compact block filters to watch for transactions to your addresses without downloading full blocks. This is how mobile Bitcoin wallets work.

**Node debugging tool** — connect to a local `regtest` Bitcoin Core instance. Inject specific messages, simulate edge cases, test how the node responds to malformed or out-of-order messages.

---

## Key concepts this project covers

- Bitcoin's P2P network topology and gossip propagation
- TCP as a byte stream vs. a message protocol, and why exact reads matter
- Binary serialization: little-endian integers, varints, varstrings, network addresses
- The double-SHA256 checksum scheme and why Bitcoin uses it
- The version handshake and why it's required before any other message
- IPv4-mapped IPv6 address encoding and why port fields are big-endian
- Magic bytes as network identifiers and parser resync anchors
- The `inv` → `getdata` → data exchange pattern that drives block and transaction propagation

---

## Network

Connects to **testnet3** by default (port 18333). To switch to mainnet, change one constant in `main.rs`:

```rust
// testnet (default)
let magic = MAGIC_TESTNET;

// mainnet
let magic = MAGIC_MAINNET;
```

And change the DNS seeds in `network.rs` to mainnet seeds (`seed.bitcoin.sipa.be`, etc.) and port `8333`.

---

## License

MIT

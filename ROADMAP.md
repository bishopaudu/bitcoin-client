# Bitcoin P2P Client — Feature Audit & Roadmap

## What We Currently Have

### Core Rust Backend
| Feature | File | Status |
|---|---|---|
| DNS seed discovery (find testnet peers) | `src/network.rs` | ✅ Working |
| TCP peer connection with timeout | `src/peer.rs` | ✅ Working |
| Bitcoin P2P handshake (version/verack) | `src/node.rs`, `src/version.rs` | ✅ Working |
| Keepalive (ping/pong) | `src/node.rs` | ✅ Working |
| Background thread (non-blocking UI) | `src/node.rs` | ✅ Working |
| Live message parsing (inv, tx, block, addr...) | `src/parser.rs`, `src/node.rs` | ✅ Working |
| Mempool snapshot (`mempool` + `inv` response) | `src/mempool.rs`, `src/command.rs` | ✅ Working |
| Transaction fetch (`getdata` with `MSG_WITNESS_TX`) | `src/command.rs` | ✅ Working |
| Transaction parsing (SegWit + Legacy) | `src/transaction.rs` | ✅ Working |
| Script classification (P2PKH, P2SH, P2WPKH, P2WSH) | `src/node.rs` | ✅ Working |
| Graceful disconnect | `src/command.rs` | ✅ Working |
| `notfound` event forwarding to UI | `src/node.rs` | ✅ Working |

### React Frontend
| Feature | Status |
|---|---|
| Connection status indicator (dot + text) | ✅ Working |
| Live message log (auto-scrolling terminal) | ✅ Working |
| Peer info panel (agent, height, protocol, services) | ✅ Working |
| Transaction TXID input + Fetch button | ✅ Working |
| Mempool snapshot with clickable TXID table | ✅ Working |
| Transaction detail Modal (inputs, outputs, SegWit badge) | ✅ Working |
| 5-second timeout + alert if peer ghosts us | ✅ Working |
| Outgoing `getdata` logged in live feed | ✅ Working |

---

## What We Could Add — Ordered by Difficulty

### 🟢 Easy (1–3 days each)

**1. Message Log Filtering & Search**
Add a filter bar above the log. Let the user type "tx" or "ping" to show only those message types. Add a checkbox to hide keepalive (ping/pong) noise.
- *Files:* `App.jsx` only. Pure React state, no Rust changes.

**2. Copy-to-Clipboard Buttons**
Add a small clipboard icon next to the TXID in the modal and the peer address in the peer info panel. Clicking it copies the value to the clipboard.
- *Files:* `App.jsx`, `App.css` only.

**3. Transaction Count Badge**
Show a live running count of "X transactions seen" next to the message log title as `tx` messages arrive.
- *Files:* `App.jsx` only.

**4. Mempool Sort & Filter**
Allow the user to sort the mempool TXID list alphabetically or filter by typing part of a TXID.
- *Files:* `App.jsx` only.

**5. Dark/Light Mode Toggle**
Add a sun/moon toggle button in the top bar that switches the CSS theme variables between dark and light palettes.
- *Files:* `App.jsx`, `App.css` only.

---

### 🟡 Medium (1–2 weeks each)

**6. Multi-Peer Connection**
Connect to 3–5 peers simultaneously instead of just 1. Show each peer's data in a tabbed or side-by-side panel.
- *Files:* `src/node.rs`, `src/command.rs`, `App.jsx`. Requires refactoring `SharedState` into a Vec of peers.

**7. Transaction History (Session Log)**
Keep a running list of every transaction successfully decoded during the session. Display them in a scrollable "History" tab so the user can go back and view any transaction they fetched.
- *Files:* `App.jsx` (store `txData` as array, add a History tab).

**8. Block Header Monitor**
After sending `getheaders`, parse the incoming `headers` message and display a live feed of the latest blocks: hash, height, timestamp, and difficulty target.
- *Files:* `src/node.rs` (parse headers payload), `App.jsx` (new Block panel).

**9. Fee Rate Dashboard**
Parse the `feefilter` message (which we already receive) and any `inv`→`tx` flow to calculate a running average sat/vB for transactions seen. Display a "Current Feerate" mini-chart.
- *Files:* `src/node.rs` (extract tx size + fee), `src/transaction.rs`, `App.jsx`.

**10. Clickable Mempool → Auto Fetch**
Right now clicking a mempool TXID fills the input box. Upgrade it so clicking directly fetches the transaction AND opens the modal automatically, without requiring a second button press.
- *Files:* `App.jsx` only. Medium because of state coordination between mempool click and fetch flow.

---

### 🔴 Hard (weeks to months each)

**11. `txindex` Node Selection / Node Picker**
Let the user manually input a specific node IP to connect to (e.g., one they control with `txindex=1` enabled), which would allow fetching any historical transaction, not just unconfirmed ones.
- *Files:* `src/command.rs`, `src/network.rs`, `App.jsx` (add IP input field).

**12. SPV Block Verification (Merkle Proofs)**
Download block headers and use Merkle proofs to verify that a given transaction is genuinely included in a confirmed block without downloading the full block data. This is the basis of how Bitcoin lightweight wallets work.
- *Files:* Major new modules — `src/spv.rs`, `src/merkle.rs`. Heavy Rust cryptography.

**13. Address Balance Tracker (UTXO Set)**
Let the user input a Bitcoin address and track every `tx` that comes through the live feed that mentions that address in an input or output. Build a real-time "balance delta" tracker.
- *Files:* `src/transaction.rs` (address extraction), new `src/tracker.rs`, `App.jsx` (Watchlist panel).

**14. Transaction Broadcasting**
Allow the user to paste a raw signed transaction hex and broadcast it to the connected peer using a `tx` message. This turns the app into a basic transaction broadcaster.
- *Files:* New `src/broadcast.rs`, `src/command.rs` (new `broadcast_tx` command), `App.jsx`.

**15. Full Wallet (Keys + Signing)**
Generate a private/public key pair, derive a P2WPKH address, construct and sign transactions using SIGHASH_ALL. This is a full wallet implementation and is one of the most complex features in all of Bitcoin development.
- *Files:* Major new modules — `src/wallet.rs`, `src/script.rs`, `src/signing.rs`. Requires adding `secp256k1` cryptography library.

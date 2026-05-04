// mempool.rs — Mempool Snapshot: Request and Display All Pending Transactions
//
// ── What is the Mempool? ─────────────────────────────────────────────────────
//
// Every full Bitcoin node keeps a "mempool" (memory pool) — an in-RAM waiting
// room of validated but unconfirmed transactions. Think of it as the queue
// before the checkout (the blockchain).
//
// The lifecycle of a transaction:
//
//   1. Alice broadcasts her signed transaction to the network
//   2. Every node that receives it validates it (correct signatures, not double-spend)
//   3. Valid transactions are held in the mempool
//   4. Miners pick transactions from the mempool — usually highest fee rate first
//   5. The chosen transactions are bundled into a new block and mined
//   6. Once confirmed in a block, the transactions leave the mempool permanently
//
// Key facts about mempools:
//   - Every node's mempool is slightly different (nodes hear txs at different times)
//   - Mainnet mempools often hold thousands of transactions during busy periods
//   - Testnet mempools are much smaller — usually a few dozen to a few hundred txs
//   - Mempools are NOT stored on disk — if you restart a node, the mempool is empty
//
// ── The `mempool` P2P Message (BIP 35) ──────────────────────────────────────
//
// Bitcoin has a dedicated message for requesting a peer's mempool contents.
// It was added in BIP 35 (Bitcoin Improvement Proposal number 35).
//
// The `mempool` message has ZERO payload bytes. It's just a signal.
// Sending it is like asking: "Hey peer, what transactions are you currently holding?"
//
// The peer responds with one or more `inv` messages, each containing up to
// 50,000 txids. On testnet you'll typically get a single small inv response.
//
// ── Full Message Flow ────────────────────────────────────────────────────────
//
//   [After handshake completes]
//
//   YOU  ──► mempool        empty payload — "please share your mempool"
//              ↓
//   PEER ──► inv            list of txids currently in their mempool
//              ↓
//   YOU       [optional]    send getdata for each txid to fetch full tx data
//              ↓
//   PEER ──► tx, tx, tx...  one tx message per txid you requested
//
// ── How This Module Fits Into the Codebase ───────────────────────────────────
//
//   main.rs          — calls request_mempool_snapshot() after handshake
//   peer.rs (inv)    — calls handle_mempool_inv() when inv arrives post-mempool
//   transaction.rs   — called by this module to optionally fetch full tx data
//
// The tricky part: when we receive an `inv` in the message loop, we don't
// automatically know if it's a mempool response or a new-transaction announcement.
// We track this with a simple boolean flag `mempool_requested` in main.rs.

// (Code removed as part of API migration — mempool lookups now happen via mempool.space API)

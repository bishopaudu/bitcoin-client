// network.rs — Peer discovery via DNS seeds
//
// When a Bitcoin node starts for the first time, it has no saved peer
// addresses. To bootstrap, it performs DNS lookups on well-known hostnames
// called "DNS seeds." These hostnames are maintained by trusted community
// members and return IP addresses of currently-live Bitcoin nodes.
//
// The DNS seeds for testnet3 are hardcoded in Bitcoin Core's chainparams.cpp:
//   https://github.com/bitcoin/bitcoin/blob/master/src/kernel/chainparams.cpp
//
// Once connected to a peer, the `addr` / `getaddr` message exchange lets
// a node build up a database of many more peers, so DNS seeds are only
// needed on the very first startup.

use std::net::ToSocketAddrs;

// Known DNS seeds for Bitcoin testnet3.
// These hostnames resolve to multiple IP addresses of live testnet nodes.
// In production (mainnet), different seeds are used:
//   seed.bitcoin.sipa.be, dnsseed.bluematt.me, etc.
pub const TESTNET_DNS_SEEDS: &[&str] = &[
    "testnet-seed.bitcoin.jonasschnelli.ch", // Maintained by Jonas Schnelli (Bitcoin Core dev)
    "seed.tbtc.petertodd.org",               // Maintained by Peter Todd (Bitcoin researcher)
    "testnet-seed.bluematt.me",              // Maintained by Matt Corallo (Bitcoin Core dev)
];

// Perform a DNS lookup on a seed hostname and return the first resolved address.
//
// `hostname`: the DNS seed hostname, e.g. "testnet-seed.bluematt.me"
// `port`:     the port to pair with each resolved IP, e.g. 18333 for testnet
//
// Returns Some("ip:port") if resolution succeeds, None on failure.
//
// How it works:
//   We use Rust's standard `ToSocketAddrs` trait to perform a blocking DNS
//   lookup. The OS resolver contacts DNS servers and returns a list of A/AAAA
//   records. We take the first result and format it as "ip:port".
pub fn resolve_dns_seed(hostname: &str, port: u16) -> Option<String> {
    // Format as "hostname:port" — the ToSocketAddrs trait needs this format
    // to know which port to associate with each resolved address
    let addr_str = format!("{}:{}", hostname, port);

    match addr_str.to_socket_addrs() {
        Ok(mut addrs) => {
            // `addrs` is an iterator over all resolved addresses.
            // DNS seeds often return many IPs — we just take the first.
            if let Some(addr) = addrs.next() {
                println!("[*] DNS seed '{}' → {}", hostname, addr);
                return Some(addr.to_string()); // e.g. "203.0.113.1:18333"
            }
            // DNS lookup succeeded but returned zero records — unusual
            eprintln!("[!] DNS seed '{}' returned no addresses", hostname);
            None
        }
        Err(e) => {
            // DNS lookup failed: network unavailable, hostname not found, etc.
            eprintln!("[!] DNS lookup failed for '{}': {}", hostname, e);
            None
        }
    }
}

// Try each testnet DNS seed in order and return the first working address.
//
// We try seeds sequentially rather than in parallel for simplicity.
// If the first seed fails (it's down, DNS is unavailable, etc.),
// we fall through to the next one.
//
// Returns Some("ip:port") if any seed resolves successfully.
// Returns None if ALL seeds fail — caller should use a fallback address.
pub fn find_testnet_peer() -> Option<String> {
    for seed in TESTNET_DNS_SEEDS {
        if let Some(addr) = resolve_dns_seed(seed, 18333) {
            return Some(addr); // Return the first successful resolution
        }
    }
    // All seeds failed — network might be unavailable or seeds are down
    eprintln!("[!] All DNS seeds failed");
    None
}
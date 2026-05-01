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

pub const TESTNET_DNS_SEEDS: &[&str] = &[
    "testnet-seed.bitcoin.jonasschnelli.ch", 
    "seed.tbtc.petertodd.org",               
    "testnet-seed.bluematt.me",              
];

// Perform a DNS lookup on a seed hostname and return ALL resolved addresses.
//
// `hostname`: the DNS seed hostname, e.g. "testnet-seed.bluematt.me"
// `port`:     the port to pair with each resolved IP, e.g. 18333 for testnet
//
// Returns a Vec<String> of all "ip:port" strings resolved for this seed.
// Returns an empty Vec on DNS failure.
//
// By collecting every address, the caller can try each one in turn and skip dead peers
// without ever waiting for the OS TCP timeout (~75 seconds per attempt).
pub fn resolve_dns_seed(hostname: &str, port: u16) -> Vec<String> {
    let addr_str = format!("{}:{}", hostname, port);

    match addr_str.to_socket_addrs() {
        Ok(addrs) => {
            // Collect every resolved address, not just the first.
            // DNS seeds often return 10-50 IPs per query.
            let results: Vec<String> = addrs.map(|a| a.to_string()).collect();
            if results.is_empty() {
                eprintln!("[!] DNS seed '{}' returned no addresses", hostname);
            } else {
                println!(
                    "[*] DNS seed '{}' → {} candidate(s)",
                    hostname,
                    results.len()
                );
            }
            results
        }
        Err(e) => {
            eprintln!("[!] DNS lookup failed for '{}': {}", hostname, e);
            Vec::new()
        }
    }
}

// Collect every candidate peer address from all testnet DNS seeds.
//
// Queries every seed and merges their results into a single deduplicated list.
// The caller should iterate through this list, attempting a TCP connection
// to each address with a short timeout, stopping at the first success.
//
// Returns an empty Vec only if all DNS seeds fail entirely (no internet, etc.).
pub fn find_testnet_peers() -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for seed in TESTNET_DNS_SEEDS {
        for addr in resolve_dns_seed(seed, 18333) {
            // Deduplicate — multiple seeds may return overlapping IPs
            if seen.insert(addr.clone()) {
                candidates.push(addr);
            }
        }
    }

    if candidates.is_empty() {
        eprintln!("[!] All DNS seeds failed — no candidates found");
    } else {
        println!("[*] Total unique peer candidates: {}", candidates.len());
    }

    candidates
}
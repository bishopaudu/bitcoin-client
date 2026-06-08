// src/network.rs — Bitcoin network peer discovery via DNS seeds

use std::net::ToSocketAddrs;

pub const TESTNET_DNS_SEEDS: &[&str] = &[
    "testnet-seed.bitcoin.jonasschnelli.ch", 
    "seed.tbtc.petertodd.org",               
    "testnet-seed.bluematt.me",              
];

// Resolves a single DNS seed to a list of IP address strings.
pub fn resolve_dns_seed(hostname: &str, port: u16) -> Vec<String> {
    let addr_str = format!("{}:{}", hostname, port);

    match addr_str.to_socket_addrs() {
        Ok(addrs) => {
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

// Queries all testnet DNS seeds and returns a deduplicated list of peer addresses.
pub fn find_testnet_peers() -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for seed in TESTNET_DNS_SEEDS {
        for addr in resolve_dns_seed(seed, 18333) {
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
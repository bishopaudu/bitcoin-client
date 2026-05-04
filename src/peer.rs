// This module owns everything related to a single peer connection:
//   - Opening the TCP socket with the right settings
//   - Sending messages down the socket
//   - Handling each incoming message type after it's been parsed
//   - Maintaining handshake state
//
// The message handler here is a dispatcher: it matches on the command
// string and routes each message to the appropriate logic. For a more
// advanced client you'd split each handler into its own function or module.

use std::net::{TcpStream, ToSocketAddrs};
use std::io::{self, Write};
use std::time::Duration;
use crate::message::build_message;

// Open a TCP connection to a Bitcoin peer and configure the socket.
//
// `addr`: a "ip:port" string, e.g. "203.0.113.1:18333"
//
// Socket configuration:
//   read_timeout  — prevents blocking forever if the peer stops sending
//   write_timeout — prevents hanging if the peer stops reading
//   nodelay       — disables Nagle's algorithm (send bytes immediately,
//                   don't buffer small writes — latency > bandwidth here)
pub fn connect_to_peer(addr: &str) -> io::Result<TcpStream> {
    println!("[*] Connecting to Bitcoin node at {}...", addr);

    // Resolve the address to a SocketAddr — required by connect_timeout.
    let socket_addr = addr
        .to_socket_addrs()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no addresses resolved"))?;

    // connect_timeout replaces the plain connect() call. The OS default TCP
    // connect timeout is ~75 seconds — far too long when iterating through a
    // list of candidates. 5 seconds is enough to confirm a peer is reachable.
    let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5))?;

    println!("[+] TCP connection established to {}", addr);

    // If we don't receive any data for 30 seconds, return a TimedOut error
    // rather than blocking forever. We'll use this to send keepalive pings.
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;

    // If a write blocks for 30 seconds (e.g. peer's receive buffer is full),
    // return an error rather than hanging indefinitely.
    stream.set_write_timeout(Some(Duration::from_secs(60)))?;

    // Disable Nagle's algorithm. Nagle buffers small writes hoping to batch them
    // into larger TCP segments for efficiency. For Bitcoin's protocol, we want
    // each message sent immediately — a delayed `verack` would stall the handshake.
    stream.set_nodelay(true)?;

    Ok(stream)
}

// Serialize and send a Bitcoin message over the TCP connection.
//
// `command`: the message type, e.g. "version", "verack", "ping", "getaddr"
// `payload`: the raw payload bytes (empty slice for messages with no body)
// `magic`:   network identifier (use MAGIC_TESTNET or MAGIC_MAINNET)
//
// We use `write_all` rather than `write` because `write` is allowed to send
// fewer bytes than requested (partial write). `write_all` loops until every
// byte has been handed to the OS, guaranteeing complete message delivery.
pub fn send_message(
    stream: &mut TcpStream,
    command: &str,
    payload: &[u8],
    magic: [u8; 4],
) -> io::Result<()> {
    // build_message combines the 24-byte header (with computed checksum)
    // and the payload into a single contiguous byte vector
    let message = build_message(command, payload, magic);

    println!(
        "[→] Sending '{}' ({} bytes payload, {} bytes total)",
        command,
        payload.len(),
        message.len()
    );

    // Write every byte of the message to the socket — no partial writes
    stream.write_all(&message)?;

    // Flush ensures the bytes leave our userspace buffer and enter the OS
    // network stack. Without this, writes might be held in a buffer.
    stream.flush()?;

    Ok(())
}



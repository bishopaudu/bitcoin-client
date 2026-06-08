// src/peer.rs — Bitcoin TCP peer connection and messaging

use std::net::{TcpStream, ToSocketAddrs};
use std::io::{self, Write};
use std::time::Duration;
use crate::message::build_message;

// Establishes a TCP connection to a Bitcoin peer with configured timeouts.
pub fn connect_to_peer(addr: &str) -> io::Result<TcpStream> {
    println!("[*] Connecting to Bitcoin node at {}...", addr);

    let socket_addr = addr
        .to_socket_addrs()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no addresses resolved"))?;

    let stream = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(5))?;

    println!("[+] TCP connection established to {}", addr);

    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    stream.set_write_timeout(Some(Duration::from_secs(60)))?;
    stream.set_nodelay(true)?;

    Ok(stream)
}

// Serializes and sends a raw Bitcoin message over the stream.
pub fn send_message(
    stream: &mut TcpStream,
    command: &str,
    payload: &[u8],
    magic: [u8; 4],
) -> io::Result<()> {
    let message = build_message(command, payload, magic);

    println!(
        "[→] Sending '{}' ({} bytes payload, {} bytes total)",
        command,
        payload.len(),
        message.len()
    );

    stream.write_all(&message)?;
    stream.flush()?;

    Ok(())
}



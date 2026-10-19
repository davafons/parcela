//! Small bounded HTTP control/readiness client; never used as a proxy.
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub fn admin(socket: &Path, method: &str, path: &str, body: &[u8]) -> Result<(u16, Vec<u8>)> {
    let mut stream = UnixStream::connect(socket)
        .context("connect to Parcela's private Caddy admin socket; start proxy run first")?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    exchange(&mut stream, method, path, "localhost", body)
}
pub fn readiness(address: SocketAddr, host: &str, path: &str) -> Result<bool> {
    Ok((200..300).contains(&status(address, host, path)?))
}
/// The status code of `GET path`, from the status line alone, within about half a second, so a
/// slow or streaming response never holds up the caller.
pub fn status(address: SocketAddr, host: &str, path: &str) -> Result<u16> {
    ensure!(
        !path.contains(['\r', '\n', ' ']) && !host.contains(['\r', '\n', ' ']),
        "invalid HTTP request target"
    );
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(150))?;
    stream.set_write_timeout(Some(Duration::from_millis(300)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )?;
    let mut head = Vec::new();
    let mut buffer = [0u8; 256];
    while !head.windows(2).any(|w| w == b"\r\n") {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        ensure!(
            !left.is_zero() && head.len() < 4096,
            "no HTTP status line in time"
        );
        stream.set_read_timeout(Some(left))?;
        let count = stream.read(&mut buffer)?;
        ensure!(count > 0, "connection closed before the HTTP status line");
        head.extend_from_slice(&buffer[..count]);
    }
    let line = String::from_utf8_lossy(&head);
    Ok(line
        .split_whitespace()
        .nth(1)
        .context("missing HTTP status")?
        .parse()?)
}
fn exchange(
    stream: &mut (impl Read + Write),
    method: &str,
    path: &str,
    host: &str,
    body: &[u8],
) -> Result<(u16, Vec<u8>)> {
    ensure!(
        !path.contains(['\r', '\n', ' ']) && !host.contains(['\r', '\n', ' ']),
        "invalid HTTP request target"
    );
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    let mut response = Vec::new();
    stream.take(4_000_001).read_to_end(&mut response)?;
    ensure!(response.len() <= 4_000_000, "HTTP response too large");
    let split = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .context("incomplete HTTP response")?;
    let header = std::str::from_utf8(&response[..split])?;
    let status: u16 = header
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .context("missing HTTP status")?
        .parse()?;
    let bytes = &response[split + 4..];
    if header
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        Ok((status, chunks(bytes)?))
    } else {
        Ok((status, bytes.to_vec()))
    }
}
fn chunks(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    loop {
        let end = bytes
            .windows(2)
            .position(|w| w == b"\r\n")
            .context("invalid chunk framing")?;
        let len = usize::from_str_radix(
            std::str::from_utf8(&bytes[..end])?
                .split(';')
                .next()
                .unwrap_or(""),
            16,
        )?;
        bytes = &bytes[end + 2..];
        if len == 0 {
            return Ok(result);
        }
        ensure!(
            len <= bytes.len().saturating_sub(2) && &bytes[len..len + 2] == b"\r\n",
            "incomplete chunk"
        );
        result.extend_from_slice(&bytes[..len]);
        bytes = &bytes[len + 2..];
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn bounded_chunk_decoder() {
        assert_eq!(
            super::chunks(b"3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n").unwrap(),
            b"abcde"
        );
        assert!(super::chunks(b"fffffff\r\nx").is_err());
    }
}

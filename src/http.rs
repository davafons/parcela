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
pub fn readiness(port: u16, host: &str, path: &str) -> Result<bool> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(150))?;
    stream.set_read_timeout(Some(Duration::from_millis(300)))?;
    stream.set_write_timeout(Some(Duration::from_millis(300)))?;
    let (code, _) = exchange(&mut stream, "GET", path, host, &[])?;
    Ok((200..300).contains(&code))
}
pub fn front_door(port: u16, marker: &str) -> Result<()> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let (code, body) = exchange(&mut stream, "GET", "/", "localhost", &[])?;
    ensure!(
        code == 404 && body == marker.as_bytes(),
        "standard front door does not match the owned router"
    );
    Ok(())
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

// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Plaintext WebSocket transport for [`NodeObservation`] batches.
//!
//! One NDJSON record per text frame: the same compact JSON line
//! [`crate::write_observation`] writes on TCP, including the trailing
//! newline. Binary frames are rejected. There is no auth. This module
//! stays plaintext; optional TLS is [`crate::obs_wss`]. Lab use only,
//! not a public endpoint.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use tungstenite::{Message, WebSocket};

use crate::node_obs::NodeObservation;

/// How long a client waits on the handshake and the close reply.
pub(crate) const CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Compact NDJSON line for one observation (JSON + `\n`).
///
/// The bytes match one [`crate::write_observation`] write, so a text frame
/// and a TCP line are the same payload.
pub fn observation_frame_text(obs: &NodeObservation) -> Result<String, String> {
    let line = serde_json::to_string(obs).map_err(|e| e.to_string())?;
    Ok(line + "\n")
}

/// Parse one WebSocket text frame as a single [`NodeObservation`].
///
/// Whitespace around the JSON value is ignored, so a frame that is exactly
/// one NDJSON line still parses. An empty frame is an error. A second JSON
/// value in the same frame is an error (`serde_json` rejects trailing data).
pub fn observation_from_ws_text(text: &str) -> Result<NodeObservation, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty WebSocket text frame".into());
    }
    NodeObservation::from_json(trimmed).map_err(|e| format!("websocket text frame: {e}"))
}

/// Accept `stream` as a WebSocket server and read text frames until close.
///
/// Each text frame is one observation. A binary frame fails the read and
/// discards any observations already taken from this socket. Ping and pong
/// are ignored (pong replies are flushed by the next read). The close
/// handshake is driven to [`tungstenite::Error::ConnectionClosed`] so the
/// peer is not left waiting on a close reply.
pub fn read_observations_from_ws_server<S>(stream: S) -> Result<Vec<NodeObservation>, String>
where
    S: Read + Write,
{
    let mut ws = tungstenite::accept(stream).map_err(|e| format!("websocket handshake: {e}"))?;
    let mut batches = Vec::new();
    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                batches.push(observation_from_ws_text(text.as_str())?);
            }
            Ok(Message::Binary(_)) => {
                return Err(
                    "binary WebSocket frames are rejected; send one NDJSON text frame per observation"
                        .into(),
                );
            }
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Close(_)) => {}
            Ok(other) => {
                return Err(format!("unsupported WebSocket message: {other}"));
            }
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                break;
            }
            Err(e) => return Err(format!("websocket read: {e}")),
        }
    }
    if batches.is_empty() {
        return Err("no observations received before close".into());
    }
    Ok(batches)
}

/// Connect to `ws://ADDR`, send one text frame, and close.
///
/// `addr` is `HOST:PORT`, the same form as TCP `--publish`. The `ws://`
/// scheme is added here. Plaintext only.
pub fn write_observation_ws_client(addr: &str, obs: &NodeObservation) -> Result<(), String> {
    write_observations_ws_client(addr, std::slice::from_ref(obs))
}

/// Connect to `ws://ADDR`, send one text frame per observation, and close.
///
/// Frames are sent on a single connection, in order. The socket is closed
/// after the last frame so the peer can finish the batch the way a TCP
/// reader finishes on EOF.
pub fn write_observations_ws_client(addr: &str, batches: &[NodeObservation]) -> Result<(), String> {
    if batches.is_empty() {
        return Err("no observations to send".into());
    }
    let url = ws_url(addr)?;
    let stream = connect_client_tcp(addr)?;
    write_observations_on_ws_client(stream, &url, batches)
}

/// Handshake and send on an already-connected stream.
///
/// `url` is `ws://HOST:PORT` or `wss://HOST:PORT`. The caller owns TLS:
/// a plaintext [`TcpStream`] or a rustls stream both implement
/// [`Read`] + [`Write`]. Binary frames are still rejected by the peer's
/// reader, not by this send path.
pub(crate) fn write_observations_on_ws_client<S>(
    stream: S,
    url: &str,
    batches: &[NodeObservation],
) -> Result<(), String>
where
    S: Read + Write,
{
    if batches.is_empty() {
        return Err("no observations to send".into());
    }
    let (mut ws, _response) =
        tungstenite::client(url, stream).map_err(|e| format!("websocket handshake {url}: {e}"))?;
    for obs in batches {
        let payload = observation_frame_text(obs)?;
        ws.send(Message::text(payload))
            .map_err(|e| format!("websocket send: {e}"))?;
    }
    finish_client_close(&mut ws)
}

pub(crate) fn connect_client_tcp(addr: &str) -> Result<TcpStream, String> {
    let stream = TcpStream::connect(addr).map_err(|e| format!("connect {addr}: {e}"))?;
    stream
        .set_nodelay(true)
        .map_err(|e| format!("nodelay {addr}: {e}"))?;
    stream
        .set_read_timeout(Some(CLIENT_IO_TIMEOUT))
        .map_err(|e| format!("read timeout {addr}: {e}"))?;
    stream
        .set_write_timeout(Some(CLIENT_IO_TIMEOUT))
        .map_err(|e| format!("write timeout {addr}: {e}"))?;
    Ok(stream)
}

pub(crate) fn require_host_port(addr: &str) -> Result<(), String> {
    if addr.is_empty() || addr.contains("://") || addr.contains('/') {
        return Err(format!("WebSocket address must be HOST:PORT (got {addr})"));
    }
    if !addr
        .rsplit_once(':')
        .is_some_and(|(_, port)| !port.is_empty())
    {
        return Err(format!("WebSocket address must be HOST:PORT (got {addr})"));
    }
    Ok(())
}

fn ws_url(addr: &str) -> Result<String, String> {
    require_host_port(addr)?;
    Ok(format!("ws://{addr}"))
}

/// Flush our close frame and read until the server drops the socket.
fn finish_client_close<S>(ws: &mut WebSocket<S>) -> Result<(), String>
where
    S: Read + Write,
{
    ws.close(None)
        .map_err(|e| format!("websocket close: {e}"))?;
    loop {
        match ws.read() {
            Ok(Message::Close(_) | Message::Ping(_) | Message::Pong(_)) => {}
            Ok(Message::Text(_) | Message::Binary(_)) => {}
            Ok(other) => {
                return Err(format!("unexpected message during close: {other}"));
            }
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Err(e) => return Err(format!("websocket close handshake: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DeviceKind, WifiDevice};
    use crate::node_obs::{DeviceObservation, NodePose};
    use crate::write_observation;
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    fn sample_obs(node_id: &str) -> NodeObservation {
        let mut d = WifiDevice::hotspot("aa:bb:cc:00:00:01", -50.0);
        d.ssid = "X".into();
        NodeObservation::new(
            node_id,
            1.0,
            2.0,
            NodePose::new(0.0, 0.0, 0.0, 0.0),
            vec![DeviceObservation::from_wifi_device(&d)],
        )
    }

    fn serve_one() -> (
        std::net::SocketAddr,
        thread::JoinHandle<Result<Vec<NodeObservation>, String>>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_observations_from_ws_server(stream)
        });
        thread::sleep(Duration::from_millis(20));
        (addr, server)
    }

    #[test]
    fn frame_text_matches_tcp_ndjson_line() {
        let obs = sample_obs("node-t");
        let mut buf = Vec::new();
        write_observation(&mut buf, &obs).unwrap();
        let frame = observation_frame_text(&obs).unwrap();
        assert_eq!(frame.as_bytes(), buf.as_slice());
        assert!(frame.ends_with('\n'));
        let back = observation_from_ws_text(&frame).unwrap();
        assert_eq!(back.node_id, "node-t");
        assert_eq!(back.device_observations[0].kind, DeviceKind::Hotspot);
    }

    #[test]
    fn empty_text_frame_is_rejected() {
        assert!(observation_from_ws_text("").unwrap_err().contains("empty"));
        assert!(observation_from_ws_text("\n")
            .unwrap_err()
            .contains("empty"));
    }

    #[test]
    fn ws_text_frame_roundtrip_client_to_server() {
        let (addr, server) = serve_one();
        let obs = sample_obs("node-ws");
        write_observation_ws_client(&addr.to_string(), &obs).unwrap();
        let got = server.join().unwrap().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].node_id, "node-ws");
        assert_eq!(got[0].device_observations.len(), 1);
        assert_eq!(got[0].device_observations[0].mac, "aa:bb:cc:00:00:01");
    }

    #[test]
    fn ws_several_text_frames_on_one_connection() {
        let (addr, server) = serve_one();
        let batches = vec![sample_obs("node-a"), sample_obs("node-b")];
        write_observations_ws_client(&addr.to_string(), &batches).unwrap();
        let got = server.join().unwrap().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].node_id, "node-a");
        assert_eq!(got[1].node_id, "node-b");
    }

    #[test]
    fn ws_binary_frame_is_rejected() {
        let (addr, server) = serve_one();
        let url = format!("ws://{addr}");
        let (mut client, _) = tungstenite::connect(url).unwrap();
        client.send(Message::binary(vec![0u8, 1, 2])).unwrap();
        let err = server.join().unwrap().unwrap_err();
        assert!(
            err.contains("binary"),
            "expected binary rejection, got {err}"
        );
        drop(client);
    }

    #[test]
    fn ws_binary_after_text_discards_the_batch() {
        let (addr, server) = serve_one();
        let url = format!("ws://{addr}");
        let (mut client, _) = tungstenite::connect(url).unwrap();
        let payload = observation_frame_text(&sample_obs("node-partial")).unwrap();
        client.send(Message::text(payload)).unwrap();
        client.send(Message::binary(b"nope".as_slice())).unwrap();
        let err = server.join().unwrap().unwrap_err();
        assert!(
            err.contains("binary"),
            "expected binary rejection, got {err}"
        );
        drop(client);
    }

    #[test]
    fn host_port_required() {
        let obs = sample_obs("node-t");
        assert!(write_observation_ws_client("ws://127.0.0.1:9", &obs).is_err());
        assert!(write_observation_ws_client("127.0.0.1", &obs).is_err());
    }
}

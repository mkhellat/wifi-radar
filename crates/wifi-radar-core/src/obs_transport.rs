//! Line-delimited JSON transport for [`NodeObservation`] over TCP.
//!
//! Passive observation exchange only. One JSON object per line (UTF-8).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

use crate::node_obs::NodeObservation;

/// Write one observation as a single JSON line (compact + `\n`).
pub fn write_observation(stream: &mut impl Write, obs: &NodeObservation) -> Result<(), String> {
    let line = serde_json::to_string(obs).map_err(|e| e.to_string())?;
    stream
        .write_all(line.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())
}

/// Read one observation line from a buffered reader.
pub fn read_observation(reader: &mut impl BufRead) -> Result<NodeObservation, String> {
    let mut line = String::new();
    let n = reader.read_line(&mut line).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("connection closed".into());
    }
    NodeObservation::from_json(line.trim_end())
}

/// Convenience: read one observation from a TCP stream.
pub fn read_observation_tcp(stream: &TcpStream) -> Result<NodeObservation, String> {
    let mut reader = BufReader::new(stream);
    read_observation(&mut reader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_obs::{DeviceObservation, NodePose};
    use crate::models::{DeviceKind, WifiDevice};
    use std::io::Cursor;
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Duration;

    fn sample_obs() -> NodeObservation {
        let mut d = WifiDevice::hotspot("aa:bb:cc:00:00:01", -50.0);
        d.ssid = "X".into();
        NodeObservation::new(
            "node-t",
            1.0,
            2.0,
            NodePose::new(0.0, 0.0, 0.0, 0.0),
            vec![DeviceObservation::from_wifi_device(&d)],
        )
    }

    #[test]
    fn line_roundtrip_via_cursor() {
        let obs = sample_obs();
        let mut buf = Vec::new();
        write_observation(&mut buf, &obs).unwrap();
        let mut cursor = Cursor::new(buf);
        let back = read_observation(&mut cursor).unwrap();
        assert_eq!(back.node_id, "node-t");
        assert_eq!(back.device_observations[0].kind, DeviceKind::Hotspot);
    }

    #[test]
    fn tcp_localhost_exchange() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let got = read_observation_tcp(&sock).unwrap();
            write_observation(&mut sock, &got).unwrap();
            got.node_id
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let obs = sample_obs();
        write_observation(&mut client, &obs).unwrap();
        let echo = read_observation_tcp(&client).unwrap();
        assert_eq!(echo.node_id, "node-t");
        assert_eq!(server.join().unwrap(), "node-t");
    }
}

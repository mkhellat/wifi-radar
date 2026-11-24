// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sensing node agent helpers: build and emit [`NodeObservation`] batches.
//!
//! Passive observation only — no association, deauth, or injection.

#![deny(missing_docs)]

use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use wifi_radar_core::{
    now_unix, write_observation, DeviceObservation, NodeObservation, NodePose, ScanBackend,
    WifiDevice,
};

/// Configuration for one observation publish.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Stable node id (hostname, label, …).
    pub node_id: String,
    /// Pose in the shared local frame.
    pub pose: NodePose,
}

impl NodeConfig {
    /// Construct a node config.
    pub fn new(node_id: impl Into<String>, pose: NodePose) -> Self {
        Self {
            node_id: node_id.into(),
            pose,
        }
    }
}

/// Build a [`NodeObservation`] from scanned devices.
pub fn observation_from_devices(
    cfg: &NodeConfig,
    devices: &[WifiDevice],
    mono_start: Instant,
) -> NodeObservation {
    let device_observations: Vec<_> = devices
        .iter()
        .map(DeviceObservation::from_wifi_device)
        .collect();
    NodeObservation::new(
        cfg.node_id.clone(),
        now_unix(),
        mono_start.elapsed().as_secs_f64(),
        cfg.pose.clone(),
        device_observations,
    )
}

/// Run one scan via a [`ScanBackend`] and build an observation batch.
pub fn scan_once(
    cfg: &NodeConfig,
    backend: &mut dyn ScanBackend,
    mono_start: Instant,
) -> Result<NodeObservation, String> {
    let devices = backend.scan().map_err(|e| e.to_string())?;
    Ok(observation_from_devices(cfg, &devices, mono_start))
}

/// Compact NDJSON line (+ newline) for TCP publish / stdout.
pub fn observation_to_ndjson(obs: &NodeObservation) -> Result<String, String> {
    let line = serde_json::to_string(obs).map_err(|e| e.to_string())?;
    Ok(line + "\n")
}

/// Connect to a fuse aggregator and write one observation as NDJSON.
pub fn publish_observation(addr: &str, obs: &NodeObservation) -> Result<(), String> {
    let mut stream = TcpStream::connect(addr).map_err(|e| format!("connect {addr}: {e}"))?;
    write_observation(&mut stream, obs)
}

/// Connect to a fuse `--listen-ws` peer and send one observation as a text frame.
///
/// `addr` is `HOST:PORT`. The frame is the same NDJSON line as
/// [`publish_observation`]. The socket is plaintext `ws://` and is closed
/// after the frame so the aggregator can finish the batch. TLS is
/// [`publish_observation_wss`].
pub fn publish_observation_ws(addr: &str, obs: &NodeObservation) -> Result<(), String> {
    wifi_radar_core::write_observation_ws_client(addr, obs)
}

/// Split a `--publish-wss` value into `HOST:PORT` and the TLS server name.
///
/// Accepts `HOST:PORT` or `wss://HOST:PORT`. The server name is the host.
/// It must be a DNS name or an IP address on the peer certificate.
pub fn parse_publish_wss_target(raw: &str) -> Result<(String, String), String> {
    let rest = raw.strip_prefix("wss://").unwrap_or(raw);
    if rest.is_empty() || rest.contains("://") || rest.contains('/') {
        return Err(format!(
            "--publish-wss needs HOST:PORT or wss://HOST:PORT (got {raw})"
        ));
    }
    let Some((host, port)) = rest.rsplit_once(':') else {
        return Err(format!(
            "--publish-wss needs HOST:PORT or wss://HOST:PORT (got {raw})"
        ));
    };
    if host.is_empty() || port.is_empty() {
        return Err(format!(
            "--publish-wss needs HOST:PORT or wss://HOST:PORT (got {raw})"
        ));
    }
    Ok((format!("{host}:{port}"), host.to_string()))
}

/// Connect to a fuse `--listen-wss` peer and send one observation as a text frame.
///
/// `addr` is `HOST:PORT`. `server_name` is checked against the certificate.
/// `tls` trusts the lab CA, which may be the server's own self-signed
/// certificate. The frame is the same NDJSON text frame as
/// [`publish_observation_ws`]. The socket closes after the frame.
pub fn publish_observation_wss(
    addr: &str,
    server_name: &str,
    tls: &wifi_radar_core::WssClientConfig,
    obs: &NodeObservation,
) -> Result<(), String> {
    wifi_radar_core::write_observation_wss_client(addr, server_name, tls, obs)
}

/// Settings for a repeated scan / publish loop.
#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// Sleep between iterations (after each successful `once` call).
    pub interval: Duration,
    /// Stop after this many iterations (`None` = run forever).
    pub count: Option<usize>,
}

impl LoopConfig {
    /// Construct a loop config.
    pub fn new(interval: Duration, count: Option<usize>) -> Self {
        Self { interval, count }
    }
}

/// Call `once` repeatedly with 0-based iteration index; sleep `interval` between runs.
///
/// Returns the number of successful iterations. Does not sleep after the final
/// iteration when `count` is reached.
pub fn run_observation_loop<F>(cfg: &LoopConfig, mut once: F) -> Result<usize, String>
where
    F: FnMut(usize) -> Result<(), String>,
{
    let mut n = 0usize;
    loop {
        once(n)?;
        n += 1;
        if cfg.count.is_some_and(|c| n >= c) {
            return Ok(n);
        }
        thread::sleep(cfg.interval);
    }
}

/// Demo / offline devices (no radio) for CI and dry runs.
pub fn demo_devices() -> Vec<WifiDevice> {
    let mut a = WifiDevice::hotspot("aa:aa:aa:00:00:01", -42.0);
    a.ssid = "Demo-AP".into();
    a.channel = 6;
    a.freq_mhz = 2437;
    let mut b = WifiDevice::hotspot("aa:aa:aa:00:00:02", -68.0);
    b.ssid = "Far-AP".into();
    b.channel = 11;
    b.freq_mhz = 2462;
    let mut c = WifiDevice::hotspot("aa:aa:aa:00:00:03", -55.0);
    c.ssid = "Mid-AP".into();
    c.channel = 1;
    c.freq_mhz = 2412;
    vec![a, b, c]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_observation_roundtrips_ndjson() {
        let cfg = NodeConfig::new("node-test", NodePose::new(0.0, 0.0, 1.0, 0.0));
        let start = Instant::now();
        let obs = observation_from_devices(&cfg, &demo_devices(), start);
        assert_eq!(obs.node_id, "node-test");
        assert_eq!(obs.device_observations.len(), 3);
        let line = observation_to_ndjson(&obs).unwrap();
        let back = NodeObservation::from_json(line.trim()).unwrap();
        assert_eq!(back.device_observations.len(), 3);
        assert_eq!(back.node_pose.x, 0.0);
    }

    #[test]
    fn publish_observation_reaches_listener() {
        use std::io::BufReader;
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;
        use wifi_radar_core::read_observation;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            read_observation(&mut reader).unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let cfg = NodeConfig::new("node-pub", NodePose::new(1.0, 2.0, 1.0, 90.0));
        let obs = observation_from_devices(&cfg, &demo_devices(), Instant::now());
        publish_observation(&addr.to_string(), &obs).unwrap();
        let got = server.join().unwrap();
        assert_eq!(got.node_id, "node-pub");
        assert_eq!(got.device_observations.len(), 3);
    }

    #[test]
    fn publish_observation_ws_reaches_listener() {
        use std::net::TcpListener;
        use std::thread;
        use std::time::Duration;
        use wifi_radar_core::read_observations_from_ws_server;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_observations_from_ws_server(stream).unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let cfg = NodeConfig::new("node-ws", NodePose::new(1.0, 2.0, 1.0, 90.0));
        let obs = observation_from_devices(&cfg, &demo_devices(), Instant::now());
        publish_observation_ws(&addr.to_string(), &obs).unwrap();
        let got = server.join().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].node_id, "node-ws");
        assert_eq!(got[0].device_observations.len(), 3);
        assert!((got[0].node_pose.heading_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn observation_loop_runs_count_times() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let hits = Arc::new(AtomicUsize::new(0));
        let hits_bg = Arc::clone(&hits);
        let cfg = LoopConfig::new(Duration::from_millis(5), Some(3));
        let n = run_observation_loop(&cfg, |_| {
            hits_bg.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn observation_loop_publishes_each_iteration() {
        use std::io::BufReader;
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use wifi_radar_core::read_observation;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let collected: Arc<Mutex<Vec<NodeObservation>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_bg = Arc::clone(&collected);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream);
                let obs = read_observation(&mut reader).unwrap();
                collected_bg.lock().unwrap().push(obs);
            }
        });
        thread::sleep(Duration::from_millis(20));
        let cfg_node = NodeConfig::new("node-loop", NodePose::new(0.0, 0.0, 1.0, 0.0));
        let loop_cfg = LoopConfig::new(Duration::from_millis(10), Some(2));
        let addr_s = addr.to_string();
        let n = run_observation_loop(&loop_cfg, |_| {
            let obs = observation_from_devices(&cfg_node, &demo_devices(), Instant::now());
            publish_observation(&addr_s, &obs)
        })
        .unwrap();
        assert_eq!(n, 2);
        server.join().unwrap();
        let got = collected.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|o| o.node_id == "node-loop"));
    }

    #[test]
    fn observation_loop_publishes_ws_each_iteration() {
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};
        use wifi_radar_core::read_observations_from_ws_server;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let collected: Arc<Mutex<Vec<NodeObservation>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_bg = Arc::clone(&collected);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let batch = read_observations_from_ws_server(stream).unwrap();
                collected_bg.lock().unwrap().extend(batch);
            }
        });
        thread::sleep(Duration::from_millis(20));
        let cfg_node = NodeConfig::new("node-ws-loop", NodePose::new(0.0, 0.0, 1.0, 0.0));
        let loop_cfg = LoopConfig::new(Duration::from_millis(10), Some(2));
        let addr_s = addr.to_string();
        let n = run_observation_loop(&loop_cfg, |_| {
            let obs = observation_from_devices(&cfg_node, &demo_devices(), Instant::now());
            publish_observation_ws(&addr_s, &obs)
        })
        .unwrap();
        assert_eq!(n, 2);
        server.join().unwrap();
        let got = collected.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|o| o.node_id == "node-ws-loop"));
    }

    #[test]
    fn parse_publish_wss_accepts_host_port_and_url() {
        let (addr, name) = parse_publish_wss_target("127.0.0.1:9801").unwrap();
        assert_eq!(addr, "127.0.0.1:9801");
        assert_eq!(name, "127.0.0.1");
        let (addr, name) = parse_publish_wss_target("wss://lab.example:443").unwrap();
        assert_eq!(addr, "lab.example:443");
        assert_eq!(name, "lab.example");
        assert!(parse_publish_wss_target("wss://127.0.0.1:9/obs").is_err());
        assert!(parse_publish_wss_target("ws://127.0.0.1:9").is_err());
        assert!(parse_publish_wss_target("127.0.0.1").is_err());
    }

    #[test]
    fn publish_observation_wss_reaches_listener() {
        use std::net::{IpAddr, Ipv4Addr, TcpListener};
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::thread;
        use std::time::Duration;

        use rcgen::{CertificateParams, KeyPair, SanType};
        use wifi_radar_core::{read_observations_from_wss_server, WssClientConfig, WssServerConfig};

        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "wifi-radar-node-wss-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let key = KeyPair::generate().unwrap();
        let mut params = CertificateParams::new(vec!["localhost".into()]).unwrap();
        params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        let cert = params.self_signed(&key).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key.serialize_pem()).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let cert_for_server = cert_path.clone();
        let key_for_server = key_path.clone();
        let server = thread::spawn(move || {
            let config = WssServerConfig::from_pem_files(&cert_for_server, &key_for_server).unwrap();
            let (stream, _) = listener.accept().unwrap();
            read_observations_from_wss_server(stream, &config).unwrap()
        });
        thread::sleep(Duration::from_millis(20));
        let client = WssClientConfig::from_ca_file(&cert_path).unwrap();
        let cfg = NodeConfig::new("node-wss", NodePose::new(1.0, 2.0, 1.0, 90.0));
        let obs = observation_from_devices(&cfg, &demo_devices(), Instant::now());
        publish_observation_wss(&addr.to_string(), "127.0.0.1", &client, &obs).unwrap();
        let got = server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].node_id, "node-wss");
        assert_eq!(got[0].device_observations.len(), 3);
    }
}

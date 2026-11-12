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
}

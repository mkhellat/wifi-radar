//! Fusion aggregator library: load observation batches and emit fused tracks.
//!
//! Passive best-effort RF localization only — not survey-grade. Do not advertise
//! product triangulation or 3D capability from this binary.

#![deny(missing_docs)]

use std::fs;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use wifi_radar_core::{
    fuse_hybrid_bearing_range_2d, fuse_multilateration_2d, fuse_multilateration_2d_robust,
    fuse_weighted_centroids, read_observation, write_observation, FuseStatus, FusedPosition2D,
    NodeObservation, DEFAULT_FUSION_WINDOW_SEC, DEFAULT_TUKEY_DELTA_M,
};

/// Which 2D solver to run over an associated epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverKind {
    /// RSSI-weighted centroid.
    Centroid,
    /// L2 multilateration.
    Multilateration,
    /// Hybrid bearing + RSSI.
    Hybrid,
    /// Tukey-robust multilateration (default delta).
    MultilaterationRobust,
}

impl SolverKind {
    /// Parse a CLI solver name (`centroid`, `mlat`, `hybrid`, `mlat-robust`).
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "centroid" => Ok(Self::Centroid),
            "mlat" | "multilateration" => Ok(Self::Multilateration),
            "hybrid" => Ok(Self::Hybrid),
            "mlat-robust" | "robust" => Ok(Self::MultilaterationRobust),
            other => Err(format!(
                "unknown solver '{other}' (expected centroid|mlat|hybrid|mlat-robust)"
            )),
        }
    }
}

/// One fused track line for stdout / NDJSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FusedTrack {
    /// Target MAC.
    pub mac: String,
    /// Estimated X (metres).
    pub x: f64,
    /// Estimated Y (metres).
    pub y: f64,
    /// Scalar confidence in \[0, 1\].
    pub confidence: f64,
    /// RMS range residual (metres).
    pub residual_rms_m: f64,
    /// Contributing node count.
    pub contributing_nodes: usize,
    /// Solver label.
    pub solver: String,
    /// Status string (`ok`).
    pub status: String,
}

impl FusedTrack {
    fn from_pos(p: &FusedPosition2D, solver: &str) -> Self {
        Self {
            mac: p.mac.clone(),
            x: p.x,
            y: p.y,
            confidence: p.confidence,
            residual_rms_m: p.residual_rms_m,
            contributing_nodes: p.contributing_nodes,
            solver: solver.to_string(),
            status: match p.status {
                FuseStatus::Ok => "ok".into(),
                FuseStatus::InsufficientNodes => "insufficient_nodes".into(),
                FuseStatus::InsufficientGeometry => "insufficient_geometry".into(),
            },
        }
    }
}

/// Load a single [`NodeObservation`] from a JSON file (pretty or compact).
pub fn load_observation_file(path: &Path) -> Result<NodeObservation, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    // Prefer a single JSON object; if that fails, try the first NDJSON line.
    match NodeObservation::from_json(text.trim()) {
        Ok(obs) => Ok(obs),
        Err(first_err) => {
            let line = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .ok_or_else(|| format!("{}: empty file ({first_err})", path.display()))?;
            NodeObservation::from_json(line)
                .map_err(|e| format!("{}: {e} (also failed whole-file: {first_err})", path.display()))
        }
    }
}

/// Load many observation files in order.
pub fn load_observation_files(paths: &[&Path]) -> Result<Vec<NodeObservation>, String> {
    paths.iter().map(|p| load_observation_file(p)).collect()
}

/// Run the selected solver over batches in one fusion window.
pub fn fuse_batches(
    batches: &[NodeObservation],
    window_sec: f64,
    solver: SolverKind,
) -> Vec<FusedTrack> {
    let window = if window_sec > 0.0 {
        window_sec
    } else {
        DEFAULT_FUSION_WINDOW_SEC
    };
    let (label, positions) = match solver {
        SolverKind::Centroid => (
            "centroid",
            fuse_weighted_centroids(batches, window),
        ),
        SolverKind::Multilateration => ("mlat", fuse_multilateration_2d(batches, window)),
        SolverKind::Hybrid => ("hybrid", fuse_hybrid_bearing_range_2d(batches, window)),
        SolverKind::MultilaterationRobust => (
            "mlat-robust",
            fuse_multilateration_2d_robust(batches, window, DEFAULT_TUKEY_DELTA_M),
        ),
    };
    positions
        .iter()
        .map(|p| FusedTrack::from_pos(p, label))
        .collect()
}

/// Serialize tracks as NDJSON (one object per line, trailing newline).
pub fn tracks_to_ndjson(tracks: &[FusedTrack]) -> Result<String, String> {
    let mut out = String::new();
    for t in tracks {
        let line = serde_json::to_string(t).map_err(|e| e.to_string())?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// Parse one NDJSON track line (whitespace-trimmed; empty → error).
pub fn parse_track_ndjson_line(line: &str) -> Result<FusedTrack, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty track line".into());
    }
    serde_json::from_str(trimmed).map_err(|e| e.to_string())
}

/// Parse zero or more fused-track NDJSON lines from a blob.
///
/// Blank lines are skipped. Each non-empty line must be one [`FusedTrack`].
pub fn parse_tracks_ndjson(text: &str) -> Result<Vec<FusedTrack>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            parse_track_ndjson_line(line)
                .map_err(|e| format!("line {}: {e}", i + 1))?,
        );
    }
    Ok(out)
}

/// Accept one TCP client, read NDJSON [`NodeObservation`] lines until EOF, fuse.
///
/// Returns fused tracks. Binding address examples: `127.0.0.1:0` (ephemeral).
pub fn fuse_oneshot_tcp_listener<A: ToSocketAddrs>(
    addr: A,
    window_sec: f64,
    solver: SolverKind,
) -> Result<(std::net::SocketAddr, Vec<FusedTrack>), String> {
    let listener = TcpListener::bind(addr).map_err(|e| e.to_string())?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    let (stream, _) = listener.accept().map_err(|e| e.to_string())?;
    let batches = read_observations_until_eof(stream)?;
    Ok((local, fuse_batches(&batches, window_sec, solver)))
}

/// Read observation lines from a TCP stream until EOF / connection close.
pub fn read_observations_until_eof(stream: TcpStream) -> Result<Vec<NodeObservation>, String> {
    let mut reader = BufReader::new(stream);
    let mut batches = Vec::new();
    loop {
        match read_observation(&mut reader) {
            Ok(obs) => batches.push(obs),
            Err(e) if e == "connection closed" => break,
            Err(e) => return Err(e),
        }
    }
    if batches.is_empty() {
        return Err("no observations received before EOF".into());
    }
    Ok(batches)
}

/// Write observation batches as NDJSON to a TCP stream (helper for tests/agents).
pub fn write_observations_tcp(
    stream: &mut TcpStream,
    batches: &[NodeObservation],
) -> Result<(), String> {
    for obs in batches {
        write_observation(stream, obs)?;
    }
    Ok(())
}

/// Default idle time before fusing a continuous-listen epoch (milliseconds).
pub const DEFAULT_EPOCH_IDLE_MS: u64 = 500;

/// Settings for continuous TCP listen / idle-epoch fusion.
#[derive(Debug, Clone)]
pub struct EpochListenConfig {
    /// Association wall-clock window (seconds).
    pub window_sec: f64,
    /// Solver for each epoch.
    pub solver: SolverKind,
    /// Fuse after this much silence since the last observation.
    pub idle: Duration,
    /// Stop after this many successful fuse emissions (`None` = run forever).
    pub max_epochs: Option<usize>,
}

impl EpochListenConfig {
    /// Construct with defaults (`DEFAULT_EPOCH_IDLE_MS`, no epoch cap).
    pub fn new(window_sec: f64, solver: SolverKind) -> Self {
        Self {
            window_sec,
            solver,
            idle: Duration::from_millis(DEFAULT_EPOCH_IDLE_MS),
            max_epochs: None,
        }
    }
}

/// Bind `addr` then run [`listen_fuse_epochs_on`].
///
/// Returns the bound socket address and the number of epochs emitted.
pub fn listen_fuse_epochs<A: ToSocketAddrs, F>(
    addr: A,
    cfg: EpochListenConfig,
    emit: F,
) -> Result<(std::net::SocketAddr, usize), String>
where
    F: FnMut(&[FusedTrack]) -> Result<(), String>,
{
    let listener = TcpListener::bind(addr).map_err(|e| e.to_string())?;
    let local = listener.local_addr().map_err(|e| e.to_string())?;
    let epochs = listen_fuse_epochs_on(listener, cfg, emit)?;
    Ok((local, epochs))
}

/// Accept many TCP clients; on idle, fuse buffered observations and call `emit`.
///
/// Each client may send one or more NDJSON observations then close. Observations
/// accumulate across clients until `idle` elapses with no new data, then one
/// fuse epoch runs and the buffer is cleared. Returns the number of epochs
/// emitted (useful when `max_epochs` is set).
pub fn listen_fuse_epochs_on<F>(
    listener: TcpListener,
    cfg: EpochListenConfig,
    mut emit: F,
) -> Result<usize, String>
where
    F: FnMut(&[FusedTrack]) -> Result<(), String>,
{
    let local = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "?".into());
    let (tx, rx) = mpsc::channel::<Result<NodeObservation, String>>();
    thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let peer = stream
                        .peer_addr()
                        .map(|a| a.to_string())
                        .unwrap_or_else(|_| "?".into());
                    match read_observations_until_eof(stream) {
                        Ok(batches) => {
                            for obs in batches {
                                if tx.send(Ok(obs)).is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("client {peer}: {e}")));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("accept on {local}: {e}")));
                    return;
                }
            }
        }
    });

    let mut batches: Vec<NodeObservation> = Vec::new();
    let mut epochs = 0usize;
    loop {
        match rx.recv_timeout(cfg.idle) {
            Ok(Ok(obs)) => batches.push(obs),
            Ok(Err(e)) => return Err(e),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if batches.is_empty() {
                    continue;
                }
                let tracks = fuse_batches(&batches, cfg.window_sec, cfg.solver);
                emit(&tracks)?;
                batches.clear();
                epochs += 1;
                if cfg.max_epochs.is_some_and(|m| epochs >= m) {
                    return Ok(epochs);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                if !batches.is_empty() {
                    let tracks = fuse_batches(&batches, cfg.window_sec, cfg.solver);
                    emit(&tracks)?;
                    epochs += 1;
                }
                return Ok(epochs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../wifi-radar-core/tests/fixtures")
            .join(name)
    }

    #[test]
    fn fuse_three_node_lab_files_with_mlat() {
        let paths = [
            fixture("three_node_lab_node_a.json"),
            fixture("three_node_lab_node_b.json"),
            fixture("three_node_lab_node_c.json"),
        ];
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let batches = load_observation_files(&refs).unwrap();
        let tracks = fuse_batches(&batches, DEFAULT_FUSION_WINDOW_SEC, SolverKind::Multilateration);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].mac, "aa:aa:aa:00:00:10");
        assert_eq!(tracks[0].solver, "mlat");
        assert!((tracks[0].x - 2.0).abs() < 0.05);
        assert!((tracks[0].y - 1.0).abs() < 0.05);
    }

    #[test]
    fn solver_kind_parse() {
        assert_eq!(SolverKind::parse("mlat").unwrap(), SolverKind::Multilateration);
        assert!(SolverKind::parse("nope").is_err());
    }

    #[test]
    fn fused_track_ndjson_roundtrips() {
        let paths = [
            fixture("three_node_lab_node_a.json"),
            fixture("three_node_lab_node_b.json"),
            fixture("three_node_lab_node_c.json"),
        ];
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let batches = load_observation_files(&refs).unwrap();
        let tracks = fuse_batches(&batches, DEFAULT_FUSION_WINDOW_SEC, SolverKind::Multilateration);
        let blob = tracks_to_ndjson(&tracks).unwrap();
        let back = parse_tracks_ndjson(&blob).unwrap();
        assert_eq!(back.len(), tracks.len());
        assert_eq!(back[0].mac, tracks[0].mac);
        assert_eq!(back[0].solver, tracks[0].solver);
        assert_eq!(back[0].status, tracks[0].status);
        assert_eq!(back[0].contributing_nodes, tracks[0].contributing_nodes);
        assert!((back[0].x - tracks[0].x).abs() < 1e-9);
        assert!((back[0].y - tracks[0].y).abs() < 1e-9);
        assert!((back[0].x - 2.0).abs() < 0.05);
    }

    #[test]
    fn parse_tracks_ndjson_skips_blank_lines() {
        let line = r#"{"mac":"aa:bb:cc:00:00:01","x":1.0,"y":2.0,"confidence":0.5,"residual_rms_m":0.1,"contributing_nodes":3,"solver":"mlat","status":"ok"}"#;
        let blob = format!("\n{line}\n\n{line}\n");
        let tracks = parse_tracks_ndjson(&blob).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].mac, "aa:bb:cc:00:00:01");
    }

    #[test]
    fn oneshot_tcp_fuses_three_node_lab() {
        use std::thread;
        use std::time::Duration;

        let paths = [
            fixture("three_node_lab_node_a.json"),
            fixture("three_node_lab_node_b.json"),
            fixture("three_node_lab_node_c.json"),
        ];
        let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
        let batches = load_observation_files(&refs).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let got = read_observations_until_eof(stream).unwrap();
            fuse_batches(&got, DEFAULT_FUSION_WINDOW_SEC, SolverKind::Multilateration)
        });
        thread::sleep(Duration::from_millis(20));
        let mut client = TcpStream::connect(addr).unwrap();
        write_observations_tcp(&mut client, &batches).unwrap();
        drop(client);
        let tracks = server.join().unwrap();
        assert_eq!(tracks.len(), 1);
        assert!((tracks[0].x - 2.0).abs() < 0.05);
    }

    #[test]
    fn continuous_listen_fuses_after_idle_across_clients() {
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::time::Duration;

        let paths = [
            fixture("three_node_lab_node_a.json"),
            fixture("three_node_lab_node_b.json"),
            fixture("three_node_lab_node_c.json"),
        ];
        let batches: Vec<NodeObservation> = paths
            .iter()
            .map(|p| load_observation_file(p).unwrap())
            .collect();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let collected: Arc<Mutex<Vec<FusedTrack>>> = Arc::new(Mutex::new(Vec::new()));
        let collected_bg = Arc::clone(&collected);
        let server = thread::spawn(move || {
            let mut cfg = EpochListenConfig::new(
                DEFAULT_FUSION_WINDOW_SEC,
                SolverKind::Multilateration,
            );
            cfg.idle = Duration::from_millis(120);
            cfg.max_epochs = Some(1);
            listen_fuse_epochs_on(listener, cfg, |tracks| {
                collected_bg.lock().unwrap().extend(tracks.iter().cloned());
                Ok(())
            })
        });
        thread::sleep(Duration::from_millis(30));
        for obs in &batches {
            let mut client = TcpStream::connect(addr).unwrap();
            write_observation(&mut client, obs).unwrap();
            drop(client);
            thread::sleep(Duration::from_millis(20));
        }
        let epochs = server.join().unwrap().unwrap();
        assert_eq!(epochs, 1);
        let tracks = collected.lock().unwrap();
        assert_eq!(tracks.len(), 1);
        assert!((tracks[0].x - 2.0).abs() < 0.05);
    }
}

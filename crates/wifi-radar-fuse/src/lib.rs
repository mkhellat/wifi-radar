//! Fusion aggregator library: load observation batches and emit fused tracks.
//!
//! Passive best-effort RF localization only — not survey-grade. Do not advertise
//! product triangulation or 3D capability from this binary.

#![deny(missing_docs)]

use std::fs;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;

use serde::Serialize;
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
#[derive(Debug, Clone, Serialize, PartialEq)]
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
}

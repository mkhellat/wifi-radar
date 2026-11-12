//! wifi-radar-node — scan once and emit a NodeObservation batch.

use std::env;
use std::io::{self, Write};
use std::process;
use std::time::Instant;

use wifi_radar_core::{NodePose, ScanBackend};
use wifi_radar_linux::{IwCliBackend, Nl80211Backend};
use wifi_radar_node::{
    demo_devices, observation_from_devices, observation_to_ndjson, scan_once, NodeConfig,
};

fn usage() -> ! {
    eprintln!(
        "Usage: wifi-radar-node [options]\n\
         \n\
           --node-id ID          Node identifier (default: hostname or 'node')\n\
           --pose X,Y,Z,HEADING  Pose in shared frame (default: 0,0,1,0)\n\
           -i, --interface IFACE Wireless interface (default: wlan0)\n\
           --demo                Synthetic devices (no radio)\n\
           --pretty              Pretty-print JSON instead of NDJSON\n\
         \n\
         Scans once and writes one NodeObservation to stdout.\n\
         Passive observation only — not survey-grade localization."
    );
    process::exit(2);
}

fn main() {
    if let Err(e) = run() {
        eprintln!("wifi-radar-node: {e}");
        process::exit(1);
    }
}

fn default_node_id() -> String {
    env::var("HOSTNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| env::var("HOST").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "node".into())
}

fn parse_pose(raw: &str) -> Result<NodePose, String> {
    let parts: Vec<_> = raw.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(format!(
            "pose must be X,Y,Z,HEADING (got {raw})"
        ));
    }
    let x: f64 = parts[0].parse().map_err(|_| "bad pose X")?;
    let y: f64 = parts[1].parse().map_err(|_| "bad pose Y")?;
    let z: f64 = parts[2].parse().map_err(|_| "bad pose Z")?;
    let h: f64 = parts[3].parse().map_err(|_| "bad pose HEADING")?;
    Ok(NodePose::new(x, y, z, h))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
    }

    let mut node_id = default_node_id();
    let mut pose = NodePose::new(0.0, 0.0, 1.0, 0.0);
    let mut iface = "wlan0".to_string();
    let mut demo = false;
    let mut pretty = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--node-id" => {
                i += 1;
                node_id = args.get(i).ok_or("--node-id needs a value")?.clone();
            }
            "--pose" => {
                i += 1;
                let raw = args.get(i).ok_or("--pose needs X,Y,Z,HEADING")?;
                pose = parse_pose(raw)?;
            }
            "-i" | "--interface" => {
                i += 1;
                iface = args.get(i).ok_or("-i needs an interface")?.clone();
            }
            "--demo" => demo = true,
            "--pretty" => pretty = true,
            s if s.starts_with('-') => return Err(format!("unknown flag '{s}'")),
            s => return Err(format!("unexpected argument '{s}'")),
        }
        i += 1;
    }

    let cfg = NodeConfig::new(node_id, pose);
    let mono_start = Instant::now();
    let obs = if demo {
        observation_from_devices(&cfg, &demo_devices(), mono_start)
    } else {
        let mut nl = Nl80211Backend::new(&iface);
        match scan_once(&cfg, &mut nl as &mut dyn ScanBackend, mono_start) {
            Ok(o) => o,
            Err(nl_err) => {
                let mut iw = IwCliBackend::new(&iface);
                scan_once(&cfg, &mut iw as &mut dyn ScanBackend, mono_start).map_err(|iw_err| {
                    format!("nl80211: {nl_err}; iw fallback: {iw_err}")
                })?
            }
        }
    };

    let out = if pretty {
        obs.to_json_pretty()?
    } else {
        observation_to_ndjson(&obs)?
    };
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(out.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}

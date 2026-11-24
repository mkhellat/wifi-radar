// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! wifi-radar-node — scan and emit or publish NodeObservation batches.

use std::env;
use std::io::{self, Write};
use std::process;
use std::time::{Duration, Instant};

use wifi_radar_core::{NodePose, ScanBackend};
use wifi_radar_linux::{IwCliBackend, Nl80211Backend};
use wifi_radar_core::WssClientConfig;
use wifi_radar_node::{
    demo_devices, observation_from_devices, observation_to_ndjson, parse_publish_wss_target,
    publish_observation, publish_observation_ws, publish_observation_wss, run_observation_loop,
    scan_once, LoopConfig, NodeConfig,
};

fn usage() -> ! {
    eprintln!(
        "Usage: wifi-radar-node [options]\n\
         \n\
           --node-id ID          Node identifier (default: HOSTNAME or 'node')\n\
           --pose X,Y,Z,HEADING  Pose in shared frame (default: 0,0,1,0)\n\
           -i, --interface IFACE Wireless interface (default: wlan0)\n\
           --demo                Synthetic devices (no radio)\n\
           --pretty              Pretty-print JSON instead of NDJSON (stdout)\n\
           --publish HOST:PORT   Send NDJSON to a wifi-radar-fuse --listen peer\n\
           --publish-ws HOST:PORT  Send one NDJSON text frame to --listen-ws\n\
           --publish-wss TARGET  Send one NDJSON text frame to --listen-wss\n\
           --tls-ca PEM          Trust anchor for --publish-wss (self-signed\n\
                                 server cert is fine for a lab)\n\
           --interval SEC        Repeat scan every SEC seconds (default: once)\n\
           --count N             Stop after N iterations (requires --interval)\n\
         \n\
         TARGET is HOST:PORT or wss://HOST:PORT. The TLS name is the host.\n\
         Default: scan once; write to stdout unless --publish,\n\
         --publish-ws, or --publish-wss is set. With --interval, loop until\n\
         --count or forever.\n\
         --publish-ws is plaintext ws:// (one text frame, then close).\n\
         --publish-wss is the same text frame over TLS. Plaintext stays\n\
         the lab default.\n\
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
        return Err(format!("pose must be X,Y,Z,HEADING (got {raw})"));
    }
    let x: f64 = parts[0].parse().map_err(|_| "bad pose X")?;
    let y: f64 = parts[1].parse().map_err(|_| "bad pose Y")?;
    let z: f64 = parts[2].parse().map_err(|_| "bad pose Z")?;
    let h: f64 = parts[3].parse().map_err(|_| "bad pose HEADING")?;
    Ok(NodePose::new(x, y, z, h))
}

fn collect_observation(
    cfg: &NodeConfig,
    demo: bool,
    iface: &str,
    mono_start: Instant,
) -> Result<wifi_radar_core::NodeObservation, String> {
    if demo {
        return Ok(observation_from_devices(cfg, &demo_devices(), mono_start));
    }
    let mut nl = Nl80211Backend::new(iface);
    match scan_once(cfg, &mut nl as &mut dyn ScanBackend, mono_start) {
        Ok(o) => Ok(o),
        Err(nl_err) => {
            let mut iw = IwCliBackend::new(iface);
            scan_once(cfg, &mut iw as &mut dyn ScanBackend, mono_start).map_err(|iw_err| {
                format!("nl80211: {nl_err}; iw fallback: {iw_err}")
            })
        }
    }
}

enum Sink {
    Stdout { pretty: bool },
    Tcp(String),
    Ws(String),
    Wss {
        addr: String,
        server_name: String,
        tls: WssClientConfig,
    },
}

fn emit_observation(
    obs: &wifi_radar_core::NodeObservation,
    sink: &Sink,
) -> Result<(), String> {
    match sink {
        Sink::Tcp(addr) => {
            publish_observation(addr, obs)?;
            eprintln!(
                "wifi-radar-node: published {} device(s) as {} to {addr}",
                obs.device_observations.len(),
                obs.node_id
            );
        }
        Sink::Ws(addr) => {
            publish_observation_ws(addr, obs)?;
            eprintln!(
                "wifi-radar-node: published {} device(s) as {} via websocket to {addr}",
                obs.device_observations.len(),
                obs.node_id
            );
        }
        Sink::Wss {
            addr,
            server_name,
            tls,
        } => {
            publish_observation_wss(addr, server_name, tls, obs)?;
            eprintln!(
                "wifi-radar-node: published {} device(s) as {} via wss to {addr} ({server_name})",
                obs.device_observations.len(),
                obs.node_id
            );
        }
        Sink::Stdout { pretty } => {
            let out = if *pretty {
                obs.to_json_pretty()?
            } else {
                observation_to_ndjson(obs)?
            };
            let mut stdout = io::stdout().lock();
            stdout
                .write_all(out.as_bytes())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
    let mut publish: Option<String> = None;
    let mut publish_ws: Option<String> = None;
    let mut publish_wss: Option<String> = None;
    let mut tls_ca: Option<String> = None;
    let mut interval: Option<f64> = None;
    let mut count: Option<usize> = None;
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
            "--publish" => {
                i += 1;
                publish = Some(args.get(i).ok_or("--publish needs HOST:PORT")?.clone());
            }
            "--publish-ws" => {
                i += 1;
                publish_ws = Some(args.get(i).ok_or("--publish-ws needs HOST:PORT")?.clone());
            }
            "--publish-wss" => {
                i += 1;
                publish_wss = Some(
                    args.get(i)
                        .ok_or("--publish-wss needs HOST:PORT or wss://HOST:PORT")?
                        .clone(),
                );
            }
            "--tls-ca" => {
                i += 1;
                tls_ca = Some(args.get(i).ok_or("--tls-ca needs a PEM path")?.clone());
            }
            "--interval" => {
                i += 1;
                let raw = args.get(i).ok_or("--interval needs SEC")?;
                let sec: f64 = raw
                    .parse()
                    .map_err(|_| format!("invalid --interval '{raw}'"))?;
                if !(sec > 0.0) {
                    return Err("--interval must be > 0".into());
                }
                interval = Some(sec);
            }
            "--count" => {
                i += 1;
                let raw = args.get(i).ok_or("--count needs N")?;
                let n: usize = raw
                    .parse()
                    .map_err(|_| format!("invalid --count '{raw}'"))?;
                if n == 0 {
                    return Err("--count must be >= 1".into());
                }
                count = Some(n);
            }
            s if s.starts_with('-') => return Err(format!("unknown flag '{s}'")),
            s => return Err(format!("unexpected argument '{s}'")),
        }
        i += 1;
    }

    let publish_count = usize::from(publish.is_some())
        + usize::from(publish_ws.is_some())
        + usize::from(publish_wss.is_some());
    if publish_count > 1 {
        return Err("--publish, --publish-ws, and --publish-wss cannot be combined".into());
    }
    if publish_wss.is_none() && tls_ca.is_some() {
        return Err("--tls-ca requires --publish-wss".into());
    }
    if pretty && publish.is_some() {
        return Err("--pretty cannot be combined with --publish".into());
    }
    if pretty && publish_ws.is_some() {
        return Err("--pretty cannot be combined with --publish-ws".into());
    }
    if pretty && publish_wss.is_some() {
        return Err("--pretty cannot be combined with --publish-wss".into());
    }
    if count.is_some() && interval.is_none() {
        return Err("--count requires --interval".into());
    }

    let sink = if let Some(addr) = publish {
        Sink::Tcp(addr)
    } else if let Some(addr) = publish_ws {
        Sink::Ws(addr)
    } else if let Some(raw) = publish_wss {
        let (addr, server_name) = parse_publish_wss_target(&raw)?;
        let ca = tls_ca.ok_or("--publish-wss needs --tls-ca PEM")?;
        let tls = WssClientConfig::from_ca_file(std::path::Path::new(&ca))?;
        Sink::Wss {
            addr,
            server_name,
            tls,
        }
    } else {
        Sink::Stdout { pretty }
    };
    let cfg = NodeConfig::new(node_id, pose);
    let mono_start = Instant::now();

    if let Some(sec) = interval {
        let loop_cfg = LoopConfig::new(Duration::from_secs_f64(sec), count);
        let n = run_observation_loop(&loop_cfg, |iter| {
            let obs = collect_observation(&cfg, demo, &iface, mono_start)?;
            emit_observation(&obs, &sink)?;
            if matches!(sink, Sink::Stdout { .. }) {
                eprintln!(
                    "wifi-radar-node: iteration {iter} ({} devices)",
                    obs.device_observations.len()
                );
            }
            Ok(())
        })?;
        eprintln!("wifi-radar-node: finished {n} iteration(s)");
        return Ok(());
    }

    let obs = collect_observation(&cfg, demo, &iface, mono_start)?;
    emit_observation(&obs, &sink)
}

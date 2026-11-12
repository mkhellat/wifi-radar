//! wifi-radar-fuse — fuse observation JSON files or TCP observation streams.

use std::env;
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process;
use std::time::Duration;

use wifi_radar_core::DEFAULT_FUSION_WINDOW_SEC;
use wifi_radar_fuse::{
    fuse_batches, load_observation_files, listen_fuse_epochs_on, read_observations_until_eof,
    tracks_to_ndjson, EpochListenConfig, SolverKind, DEFAULT_EPOCH_IDLE_MS,
};

fn usage() -> ! {
    eprintln!(
        "Usage:\n\
           wifi-radar-fuse [--solver centroid|mlat|hybrid|mlat-robust] [--window SEC] FILE...\n\
           wifi-radar-fuse --listen HOST:PORT [--continuous|--oneshot] [--idle-ms MS] ...\n\
         \n\
         File mode: load NodeObservation JSON files and print fused 2D tracks.\n\
         Listen mode (default continuous): accept many TCP clients; after\n\
         --idle-ms silence, fuse buffered observations and print tracks.\n\
         --oneshot: accept one client, fuse once, exit.\n\
         Passive best-effort only — not survey-grade localization."
    );
    process::exit(2);
}

fn main() {
    if let Err(e) = run() {
        eprintln!("wifi-radar-fuse: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
    }

    let mut solver = SolverKind::Multilateration;
    let mut window = DEFAULT_FUSION_WINDOW_SEC;
    let mut listen: Option<String> = None;
    let mut oneshot = false;
    let mut continuous = false;
    let mut idle_ms = DEFAULT_EPOCH_IDLE_MS;
    let mut files: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--solver" => {
                i += 1;
                let name = args.get(i).ok_or("--solver needs a value")?;
                solver = SolverKind::parse(name)?;
            }
            "--window" => {
                i += 1;
                let raw = args.get(i).ok_or("--window needs a value")?;
                window = raw
                    .parse::<f64>()
                    .map_err(|_| format!("invalid --window '{raw}'"))?;
            }
            "--listen" => {
                i += 1;
                let addr = args.get(i).ok_or("--listen needs HOST:PORT")?;
                listen = Some(addr.clone());
            }
            "--oneshot" => oneshot = true,
            "--continuous" => continuous = true,
            "--idle-ms" => {
                i += 1;
                let raw = args.get(i).ok_or("--idle-ms needs a value")?;
                idle_ms = raw
                    .parse::<u64>()
                    .map_err(|_| format!("invalid --idle-ms '{raw}'"))?;
            }
            s if s.starts_with('-') => {
                return Err(format!("unknown flag '{s}'"));
            }
            s => files.push(s.to_string()),
        }
        i += 1;
    }

    if oneshot && continuous {
        return Err("use only one of --oneshot or --continuous".into());
    }

    if let Some(addr) = listen {
        if !files.is_empty() {
            return Err("FILE arguments cannot be combined with --listen".into());
        }
        // Default listen mode is continuous; --oneshot keeps the old behavior.
        if oneshot {
            let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
            let local = listener.local_addr().map_err(|e| e.to_string())?;
            eprintln!("wifi-radar-fuse: listening on {local} (oneshot)");
            let (stream, peer) = listener.accept().map_err(|e| e.to_string())?;
            eprintln!("wifi-radar-fuse: accepted {peer}");
            let batches = read_observations_until_eof(stream)?;
            let tracks = fuse_batches(&batches, window, solver);
            let out = tracks_to_ndjson(&tracks)?;
            print!("{out}");
            return Ok(());
        }

        let mut cfg = EpochListenConfig::new(window, solver);
        cfg.idle = Duration::from_millis(idle_ms);
        let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| e.to_string())?;
        eprintln!("wifi-radar-fuse: continuous listen on {local} (idle {idle_ms} ms)");
        let epochs = listen_fuse_epochs_on(listener, cfg, |tracks| {
            let out = tracks_to_ndjson(tracks)?;
            let mut stdout = io::stdout().lock();
            stdout
                .write_all(out.as_bytes())
                .map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
            eprintln!("wifi-radar-fuse: epoch {} track(s)", tracks.len());
            Ok(())
        })?;
        eprintln!("wifi-radar-fuse: stopped after {epochs} epoch(s) ({local})");
        return Ok(());
    }

    if files.is_empty() {
        return Err("no observation FILE arguments (or use --listen)".into());
    }
    let paths: Vec<&Path> = files.iter().map(Path::new).collect();
    let batches = load_observation_files(&paths)?;
    let tracks = fuse_batches(&batches, window, solver);
    let out = tracks_to_ndjson(&tracks)?;
    print!("{out}");
    Ok(())
}

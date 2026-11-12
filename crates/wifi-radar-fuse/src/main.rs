//! wifi-radar-fuse — fuse observation JSON files or a oneshot TCP stream.

use std::env;
use std::net::TcpListener;
use std::path::Path;
use std::process;

use wifi_radar_core::DEFAULT_FUSION_WINDOW_SEC;
use wifi_radar_fuse::{
    fuse_batches, load_observation_files, read_observations_until_eof, tracks_to_ndjson,
    SolverKind,
};

fn usage() -> ! {
    eprintln!(
        "Usage:\n\
           wifi-radar-fuse [--solver centroid|mlat|hybrid|mlat-robust] [--window SEC] FILE...\n\
           wifi-radar-fuse --listen HOST:PORT [--solver ...] [--window SEC]\n\
         \n\
         File mode: load NodeObservation JSON files and print fused 2D tracks.\n\
         Listen mode: accept one TCP client, read NDJSON observations until EOF,\n\
         fuse once, print tracks, exit.\n\
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
            s if s.starts_with('-') => {
                return Err(format!("unknown flag '{s}'"));
            }
            s => files.push(s.to_string()),
        }
        i += 1;
    }

    let batches = if let Some(addr) = listen {
        if !files.is_empty() {
            return Err("FILE arguments cannot be combined with --listen".into());
        }
        let listener = TcpListener::bind(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| e.to_string())?;
        eprintln!("wifi-radar-fuse: listening on {local} (oneshot)");
        let (stream, peer) = listener.accept().map_err(|e| e.to_string())?;
        eprintln!("wifi-radar-fuse: accepted {peer}");
        read_observations_until_eof(stream)?
    } else {
        if files.is_empty() {
            return Err("no observation FILE arguments (or use --listen)".into());
        }
        let paths: Vec<&Path> = files.iter().map(Path::new).collect();
        load_observation_files(&paths)?
    };

    let tracks = fuse_batches(&batches, window, solver);
    let out = tracks_to_ndjson(&tracks)?;
    print!("{out}");
    Ok(())
}

//! wifi-radar-fuse — fuse observation JSON files into NDJSON tracks.

use std::env;
use std::path::Path;
use std::process;

use wifi_radar_core::DEFAULT_FUSION_WINDOW_SEC;
use wifi_radar_fuse::{
    fuse_batches, load_observation_files, tracks_to_ndjson, SolverKind,
};

fn usage() -> ! {
    eprintln!(
        "Usage: wifi-radar-fuse [--solver centroid|mlat|hybrid|mlat-robust] [--window SEC] FILE...\n\
         \n\
         Load NodeObservation JSON files, associate within the fusion window,\n\
         and print fused 2D tracks as NDJSON on stdout.\n\
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
            s if s.starts_with('-') => {
                return Err(format!("unknown flag '{s}'"));
            }
            s => files.push(s.to_string()),
        }
        i += 1;
    }
    if files.is_empty() {
        return Err("no observation FILE arguments".into());
    }

    let paths: Vec<&Path> = files.iter().map(Path::new).collect();
    let batches = load_observation_files(&paths)?;
    let tracks = fuse_batches(&batches, window, solver);
    let out = tracks_to_ndjson(&tracks)?;
    print!("{out}");
    Ok(())
}

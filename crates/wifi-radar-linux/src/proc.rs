//! Small subprocess helpers shared by the monitor-mode code.
//!
//! `std::process::Command::output` has no timeout, and a wedged `iw` or
//! `tshark` would otherwise hang the (background) capture thread forever.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(20);

/// Run `cmd` to completion, killing it if it outlives `timeout`.
///
/// stdin is `/dev/null`; stdout and stderr are captured. Returns
/// [`io::ErrorKind::TimedOut`] after killing the child on timeout.
pub(crate) fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> io::Result<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let out_reader = drain(child.stdout.take());
    let err_reader = drain(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => break None,
            None => thread::sleep(POLL),
        }
    };
    let timed_out = status.is_none();
    if timed_out {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    match status {
        Some(status) => Ok(Output {
            status,
            stdout,
            stderr,
        }),
        None => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("command timed out after {}s", timeout.as_secs()),
        )),
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_of_fast_command() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2"]);
        let o = run_with_timeout(&mut cmd, Duration::from_secs(5)).unwrap();
        assert!(o.status.success());
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "out");
        assert_eq!(String::from_utf8_lossy(&o.stderr).trim(), "err");
    }

    #[test]
    fn kills_command_that_outlives_timeout() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let t0 = Instant::now();
        let err = run_with_timeout(&mut cmd, Duration::from_millis(150)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn missing_program_is_spawn_error() {
        let mut cmd = Command::new("wifi-radar-no-such-program");
        let err = run_with_timeout(&mut cmd, Duration::from_secs(1)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}

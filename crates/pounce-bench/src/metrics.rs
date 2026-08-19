//! Spawns a child process and samples its resident memory while it runs.
//!
//! Deliberately agnostic about what it runs. The same code measures Pounce and
//! every competitor identically, which is the only reason the resulting
//! comparison means anything — a benchmark where each tool is measured by its
//! own instrumentation is not a comparison.

use anyhow::{Context, Result, bail};
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// How often to sample the child's memory. 50ms is frequent enough to catch a
/// peak without meaningfully perturbing the measurement.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct Measurement {
    pub wall_ms: u128,
    pub peak_rss_bytes: u64,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// Runs `cmd` to completion, sampling memory throughout.
///
/// `cmd[0]` is the program, the rest are arguments. If `timeout` elapses the
/// child is killed and `timed_out` is set — a tool that cannot finish is a
/// result worth reporting, not an error to hide.
pub fn run_measured(cmd: &[String], timeout: Option<Duration>) -> Result<Measurement> {
    let Some((program, args)) = cmd.split_first() else {
        bail!("command must not be empty");
    };

    let start = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;

    let pid = Pid::from_u32(child.id());
    let mut sys = System::new();
    let mut peak_rss_bytes = 0u64;
    let mut timed_out = false;

    let exit_code = loop {
        if let Some(status) = child.try_wait().context("failed to poll child")? {
            break status.code().unwrap_or(-1);
        }

        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing().with_memory(),
        );
        if let Some(proc) = sys.process(pid) {
            // sysinfo reports memory in bytes (since 0.30).
            peak_rss_bytes = peak_rss_bytes.max(proc.memory());
        }

        if let Some(limit) = timeout
            && start.elapsed() >= limit
        {
            timed_out = true;
            let _ = child.kill();
            let status = child.wait().context("failed to reap killed child")?;
            break status.code().unwrap_or(-1);
        }

        std::thread::sleep(SAMPLE_INTERVAL);
    };

    Ok(Measurement {
        wall_ms: start.elapsed().as_millis(),
        peak_rss_bytes,
        exit_code,
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command that stays alive long enough for the sampler to observe it.
    fn sleep_cmd(secs: u32) -> Vec<String> {
        if cfg!(windows) {
            vec![
                "powershell".into(),
                "-NoProfile".into(),
                "-Command".into(),
                format!("Start-Sleep -Seconds {secs}"),
            ]
        } else {
            vec!["sh".into(), "-c".into(), format!("sleep {secs}")]
        }
    }

    fn exit_cmd(code: u8) -> Vec<String> {
        if cfg!(windows) {
            vec!["cmd".into(), "/C".into(), format!("exit {code}")]
        } else {
            vec!["sh".into(), "-c".into(), format!("exit {code}")]
        }
    }

    #[test]
    fn measures_wall_time_of_a_successful_command() {
        let m = run_measured(&sleep_cmd(1), None).unwrap();
        assert_eq!(m.exit_code, 0);
        assert!(m.wall_ms >= 900, "wall_ms was {}", m.wall_ms);
        assert!(m.wall_ms < 30_000);
        assert!(!m.timed_out);
    }

    #[test]
    fn records_nonzero_peak_memory() {
        let m = run_measured(&sleep_cmd(1), None).unwrap();
        assert!(m.peak_rss_bytes > 0, "sampler never observed the process");
    }

    #[test]
    fn propagates_a_failing_exit_code() {
        let m = run_measured(&exit_cmd(3), None).unwrap();
        assert_eq!(m.exit_code, 3);
    }

    #[test]
    fn errors_on_an_empty_command() {
        assert!(run_measured(&[], None).is_err());
    }

    #[test]
    fn errors_when_the_program_does_not_exist() {
        let cmd = vec!["definitely-not-a-real-program-xyzzy".to_string()];
        assert!(run_measured(&cmd, None).is_err());
    }

    #[test]
    fn kills_a_command_that_exceeds_its_timeout() {
        let m = run_measured(&sleep_cmd(60), Some(Duration::from_secs(2))).unwrap();
        assert!(m.timed_out, "timeout was not reported");
        assert!(m.wall_ms < 30_000, "kill took too long: {}ms", m.wall_ms);
    }
}

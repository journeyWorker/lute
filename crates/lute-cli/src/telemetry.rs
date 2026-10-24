//! Local, opt-out usage telemetry (dsl 0.43.0 §6).
//!
//! Every invocation appends one JSON line to `telemetry.jsonl` in Lute's
//! state directory: the command id, whether the run was in JSON mode, the
//! exit code, the wall time and the toolchain version — nothing else. The
//! file never leaves the machine (there is no network code here), recording
//! never changes the command's output or exit code, and every failure is
//! silent. `LUTE_TELEMETRY=0` (or `false`/`off`/`no`) or `DO_NOT_TRACK`
//! turns it off.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::envelope;

/// The file every record is appended to.
pub(crate) const FILE: &str = "telemetry.jsonl";
/// The previous file, once [`FILE`] reached [`LIMIT`].
pub(crate) const ROTATED: &str = "telemetry.jsonl.1";
/// The size at which [`FILE`] is rotated before the next record.
pub(crate) const LIMIT: u64 = 1 << 20;

/// Whether telemetry records, and where.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// Records are appended to this file.
    Enabled(PathBuf),
    /// The named environment variable turned telemetry off.
    Disabled(&'static str),
    /// No state directory could be found (no `HOME` / `LOCALAPPDATA`).
    Unavailable,
}

/// [`Status`] for this process's environment.
pub(crate) fn status() -> Status {
    status_from(|name| std::env::var_os(name))
}

/// [`Status`] for the environment `var` reads (dsl 0.43.0 §6.2, §6.3).
pub(crate) fn status_from(var: impl Fn(&str) -> Option<OsString>) -> Status {
    let value = |name: &str| var(name).map(|v| v.to_string_lossy().trim().to_ascii_lowercase());
    if value("LUTE_TELEMETRY").is_some_and(|v| matches!(v.as_str(), "0" | "false" | "off" | "no")) {
        return Status::Disabled("LUTE_TELEMETRY");
    }
    // `DO_NOT_TRACK` is compared exactly: anything but empty or `0` opts out.
    if var("DO_NOT_TRACK").is_some_and(|v| !v.is_empty() && v != "0") {
        return Status::Disabled("DO_NOT_TRACK");
    }
    match state_dir(&var) {
        Some(dir) => Status::Enabled(dir.join(FILE)),
        None => Status::Unavailable,
    }
}

/// Lute's state directory: `%LOCALAPPDATA%\lute` on Windows;
/// `$XDG_STATE_HOME/lute` when that is an absolute path, else
/// `$HOME/.local/state/lute`, elsewhere.
fn state_dir(var: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
    if cfg!(windows) {
        return absolute("LOCALAPPDATA").map(|base| base.join("lute"));
    }
    absolute("XDG_STATE_HOME")
        .map(|base| base.join("lute"))
        .or_else(|| absolute("HOME").map(|home| home.join(".local").join("state").join("lute")))
}

/// The number of an exit code (`ExitCode` exposes none).
fn exit_number(code: ExitCode) -> u8 {
    (0..=u8::MAX)
        .find(|n| ExitCode::from(*n) == code)
        .unwrap_or(1)
}

/// One record (dsl 0.43.0 §6.1).
fn record_line(
    command: Option<&str>,
    json: bool,
    exit: u8,
    elapsed: Duration,
    now: SystemTime,
) -> String {
    let ts = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    let duration = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let command = command.map_or(Value::Null, |c| Value::String(c.into()));
    let mut line = json!({
        "v": 1,
        "ts": ts,
        "lute": env!("CARGO_PKG_VERSION"),
        "command": command,
        "json": json,
        "exit": exit,
        "durationMs": duration,
    })
    .to_string();
    line.push('\n');
    line
}

/// Record the invocation `args` that ended with `code` after `elapsed`,
/// when telemetry is enabled. Never fails and never prints.
pub(crate) fn record(args: &[OsString], code: ExitCode, elapsed: Duration) {
    let Status::Enabled(file) = status() else {
        return;
    };
    let command = envelope::command()
        .map(str::to_string)
        .or_else(|| envelope::command_id_in(args));
    let line = record_line(
        command.as_deref(),
        envelope::json_mode_in(args),
        exit_number(code),
        elapsed,
        SystemTime::now(),
    );
    let _ = append(&file, &line);
}

/// Append `line` to `file` with one write, rotating `file` to [`ROTATED`]
/// first when it has reached [`LIMIT`] (dsl 0.43.0 §6.4). Best-effort: no
/// lock is taken.
fn append(file: &Path, line: &str) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    if std::fs::metadata(file).is_ok_and(|m| m.len() >= LIMIT) {
        let _ = std::fs::rename(file, file.with_file_name(ROTATED));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(line.as_bytes())
}

/// How to turn telemetry off, for the doctor hint.
pub(crate) const OPT_OUT: &str = "LUTE_TELEMETRY=0";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn opt_out_variables_disable_recording() {
        for value in ["0", "false", "OFF", " no "] {
            assert_eq!(
                status_from(env(&[("LUTE_TELEMETRY", value), ("HOME", "/h")])),
                Status::Disabled("LUTE_TELEMETRY"),
                "{value}"
            );
        }
        assert_eq!(
            status_from(env(&[("DO_NOT_TRACK", "1"), ("HOME", "/h")])),
            Status::Disabled("DO_NOT_TRACK")
        );
        assert_eq!(
            status_from(env(&[("DO_NOT_TRACK", "true"), ("HOME", "/h")])),
            Status::Disabled("DO_NOT_TRACK")
        );
        assert_eq!(
            status_from(env(&[("DO_NOT_TRACK", " 0 "), ("HOME", "/h")])),
            Status::Disabled("DO_NOT_TRACK")
        );
        for kept in ["0", ""] {
            assert!(matches!(
                status_from(env(&[("DO_NOT_TRACK", kept), ("HOME", "/h")])),
                Status::Enabled(_)
            ));
        }
        assert!(matches!(
            status_from(env(&[("LUTE_TELEMETRY", "1"), ("HOME", "/h")])),
            Status::Enabled(_)
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn the_state_directory_follows_xdg_then_home() {
        assert_eq!(
            status_from(env(&[("XDG_STATE_HOME", "/s"), ("HOME", "/h")])),
            Status::Enabled(PathBuf::from("/s/lute/telemetry.jsonl"))
        );
        assert_eq!(
            status_from(env(&[("XDG_STATE_HOME", "relative"), ("HOME", "/h")])),
            Status::Enabled(PathBuf::from("/h/.local/state/lute/telemetry.jsonl"))
        );
        assert_eq!(status_from(env(&[])), Status::Unavailable);
        assert_eq!(
            status_from(env(&[("HOME", "relative")])),
            Status::Unavailable
        );
    }

    #[test]
    fn a_record_holds_exactly_the_specified_fields() {
        let line = record_line(
            Some("scenario reach"),
            true,
            2,
            Duration::from_millis(12),
            UNIX_EPOCH + Duration::from_millis(5),
        );
        assert!(line.ends_with('\n'));
        let value: Value = serde_json::from_str(&line).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["command", "durationMs", "exit", "json", "lute", "ts", "v"]
        );
        assert_eq!(value["command"], "scenario reach");
        assert_eq!(value["exit"], 2);
        assert_eq!(value["ts"], 5);
        assert_eq!(value["durationMs"], 12);
        let unknown: Value =
            serde_json::from_str(&record_line(None, false, 0, Duration::ZERO, UNIX_EPOCH)).unwrap();
        assert_eq!(unknown["command"], Value::Null);
    }

    #[test]
    fn exit_numbers_round_trip() {
        for n in [0u8, 1, 2, 3, 101] {
            assert_eq!(exit_number(ExitCode::from(n)), n);
        }
    }

    #[test]
    fn append_rotates_at_the_limit_and_keeps_lines_whole() {
        let dir = std::env::temp_dir().join(format!("lute-telemetry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("lute").join(FILE);
        append(&file, "{\"a\":1}\n").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"a\":1}\n");
        std::fs::write(&file, vec![b'x'; usize::try_from(LIMIT).unwrap()]).unwrap();
        append(&file, "{\"b\":2}\n").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{\"b\":2}\n");
        assert_eq!(
            std::fs::metadata(file.with_file_name(ROTATED))
                .unwrap()
                .len(),
            LIMIT
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unwritable_directory_is_an_error_the_caller_ignores() {
        let blocker =
            std::env::temp_dir().join(format!("lute-telemetry-block-{}", std::process::id()));
        std::fs::write(&blocker, "a file, not a directory").unwrap();
        assert!(append(&blocker.join("lute").join(FILE), "{}\n").is_err());
        let _ = std::fs::remove_file(&blocker);
    }
}

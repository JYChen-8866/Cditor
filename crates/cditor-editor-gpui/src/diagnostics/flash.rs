//! Frame-flash investigation tracing.
//!
//! Enabled with `CDITOR_TRACE_FLASH=1`. Logs every event that can visibly
//! change an already-rendered block between frames: per-block skeleton
//! fallbacks, code-highlight cache misses/evictions, and worker-permit
//! denials. Pair with the runtime-side `[cditor][flash][runtime]` frame
//! summaries (same env var) to correlate a visual flash with the projection
//! decision that produced it.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_FLASH")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

pub(crate) fn trace(event: &str, details: fmt::Arguments<'_>) {
    if enabled() {
        super::stderr::write(format_args!("[cditor][flash][gui][{event}] {details}"));
    }
}

/// Per-key state trace: logs only when the formatted state for `key` differs
/// from the previous frame, so per-frame render-path probes stay readable.
/// A visual flash shows up as a state line followed by its reversal.
pub(crate) fn trace_state(event: &'static str, key: u64, details: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }
    let line = format!("[cditor][flash][gui][{event}] {details}");
    static LAST: OnceLock<Mutex<HashMap<(&'static str, u64), String>>> = OnceLock::new();
    let Ok(mut last) = LAST.get_or_init(|| Mutex::new(HashMap::new())).lock() else {
        super::stderr::write(format_args!("{line}"));
        return;
    };
    if last.get(&(event, key)).map(String::as_str) == Some(line.as_str()) {
        return;
    }
    super::stderr::write(format_args!("{line}"));
    last.insert((event, key), line);
}

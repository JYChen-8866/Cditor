#[cfg(test)]
pub(crate) fn pause_at_commit_point(point: &str) {
    if std::env::var("CDITOR_SQLITE_KILL_POINT").ok().as_deref() != Some(point) {
        return;
    }
    let marker = std::env::var_os("CDITOR_SQLITE_KILL_MARKER")
        .expect("fault-injection marker path must be configured");
    std::fs::write(marker, point).expect("fault-injection marker must be writable");
    loop {
        std::thread::park();
    }
}

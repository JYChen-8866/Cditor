use std::fmt;
use std::sync::OnceLock;

pub(crate) fn trace(event: &str, details: fmt::Arguments<'_>) {
    if enabled() {
        super::stderr::write(format_args!(
            "[cditor][image-resize][gui][{event}] {details}"
        ));
    }
}

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("CDITOR_TRACE_IMAGE_RESIZE")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

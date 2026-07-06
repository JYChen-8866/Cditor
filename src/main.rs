use std::env;

use CDitor_V2::{Cditor, runtime::LARGE_MIXED_DEMO_DOCUMENT_ID};
use gpui::*;

fn main() {
    let app = gpui_platform::application();
    app.run(|cx: &mut App| {
        cx.activate(true);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Point::default(),
                    size: Size {
                        width: px(1120.0),
                        height: px(780.0),
                    },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("CDitor V2".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| cditor_from_env().build_view(cx)),
        )
        .expect("open CDitor V2 window");
    });
}

fn cditor_from_env() -> Cditor {
    let mut cditor = if env_flag("CDITOR_SMALL_DEMO", false) {
        Cditor::new().demo()
    } else {
        Cditor::new().large_demo()
    }
    .with_debug_overlay(env_flag("CDITOR_DEBUG_OVERLAY", true))
    .with_readonly(env_flag("CDITOR_READONLY", false));

    if let Some(size) = env_usize("CDITOR_PAYLOAD_WINDOW_SIZE") {
        cditor = cditor.with_payload_window_size(size);
    }

    if let Some(workspace_id) = env_u64("CDITOR_WORKSPACE_ID") {
        cditor = cditor.with_workspace_id(workspace_id);
    }

    let seed_large_demo = env_flag("CDITOR_SEED_LARGE_DEMO", false);
    let force_reseed = env_flag("CDITOR_FORCE_RESEED_LARGE_DEMO", false);
    let seed_block_count = env_usize("CDITOR_SEED_LARGE_DEMO_BLOCKS")
        .unwrap_or(CDitor_V2::runtime::LARGE_MIXED_DEMO_BLOCKS);

    match (
        env::var("CDITOR_DATABASE_URL").ok(),
        env_u64("CDITOR_DOCUMENT_ID"),
    ) {
        (Some(database_url), Some(document_id)) => {
            let cditor = cditor
                .with_document_id(document_id)
                .with_postgres_url(database_url);
            if seed_large_demo {
                cditor.with_postgres_large_demo_seed(seed_block_count, force_reseed)
            } else {
                cditor
            }
        }
        (Some(database_url), None) if seed_large_demo => cditor
            .with_document_id(LARGE_MIXED_DEMO_DOCUMENT_ID)
            .with_postgres_url(database_url)
            .with_postgres_large_demo_seed(seed_block_count, force_reseed),
        (Some(_), None) => {
            eprintln!(
                "CDITOR_DATABASE_URL is set but CDITOR_DOCUMENT_ID is missing; falling back to demo document"
            );
            cditor
        }
        _ => cditor,
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse().ok()
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

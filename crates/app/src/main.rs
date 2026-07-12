use std::env;

use cditor_app::Cditor;
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
                        width: px(1200.0),
                        height: px(800.0),
                    },
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some("Cditor".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| cditor_from_env().build_view(cx)),
        )
        .expect("open Cditor window");
    });
}

fn cditor_from_env() -> Cditor {
    let mut cditor = if env_flag("CDITOR_LARGE_DEMO", false) {
        Cditor::new().large_demo()
    } else if env_flag("CDITOR_SMALL_DEMO", false) {
        Cditor::new().demo()
    } else {
        Cditor::new().memory()
    }
    .with_debug_overlay(env_flag("CDITOR_DEBUG_OVERLAY", false))
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
        .unwrap_or(cditor_app::runtime::LARGE_MIXED_DEMO_BLOCKS);

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
        (Some(database_url), None) => {
            let cditor = cditor.with_document_id(1).with_postgres_url(database_url);
            if seed_large_demo {
                cditor.with_postgres_large_demo_seed(seed_block_count, force_reseed)
            } else {
                cditor
            }
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

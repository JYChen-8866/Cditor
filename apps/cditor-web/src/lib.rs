use cditor_editor_gpui::{CditorV2View, bind_cditor_keys};
use cditor_runtime::DocumentRuntime;
use gpui::*;
use wasm_bindgen::prelude::*;

// ── Cditor web view ─────────────────────────────────────────────────

struct CditorWebView {
    view: Entity<CditorV2View>,
    _blink_sub: Subscription,
}

impl CditorWebView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let view = cx.new(|cx| {
            CditorV2View::from_runtime_with_options(DocumentRuntime::demo(), false, false, cx)
        });
        let blink_entity = view.read(cx).caret_blink_entity().clone();
        let blink_sub = window.observe(
            &blink_entity,
            cx,
            |_: Entity<cditor_editor_gpui::CaretBlink>, window: &mut Window, _: &mut App| {
                window.refresh()
            },
        );
        Self {
            view,
            _blink_sub: blink_sub,
        }
    }
}

impl Render for CditorWebView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().size_full().child(self.view.clone())
    }
}

// ── WASM entry point ────────────────────────────────────────────────

#[wasm_bindgen]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).expect("initialize console logger");

    gpui_platform::web_init();

    // `Application::run` only clones its inner `Rc<AppCell>` into the closure that
    // `WebPlatform::run` hands to `spawn_local`. That closure is dropped as soon as
    // `on_finish_launching` returns, so on wasm the whole app — windows, entities,
    // renderer — is torn down the moment `run()` finishes and nothing ever paints.
    // Native platforms don't hit this because `platform.run` never returns.
    // Leak one extra strong reference to keep the app alive for the page's lifetime.
    let app = {
        let app = gpui_platform::single_threaded_web();
        struct WasmApplication(std::rc::Rc<AppCell>);
        let wasm_app = unsafe { std::mem::transmute::<Application, WasmApplication>(app) };
        std::mem::forget(wasm_app.0.clone());
        unsafe { std::mem::transmute::<WasmApplication, Application>(wasm_app) }
    };

    app.run(|cx: &mut App| {
        bind_cditor_keys(cx);
        cx.activate(true);

        cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| CditorWebView::new(window, cx))
        })
        .expect("open Cditor Web window");
    });

    Ok(())
}

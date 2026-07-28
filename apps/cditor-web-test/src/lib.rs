use gpui::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn run() {
    gpui_platform::web_init();
    let app = {
        let app = gpui_platform::single_threaded_web();

        // Match gpui-component's WASM lifetime workaround. WebPlatform::run
        // initializes WebGPU asynchronously, so the AppCell must outlive this
        // start function and remain available to the browser callbacks.
        struct WasmApplication(std::rc::Rc<AppCell>);
        let wasm_app = unsafe { std::mem::transmute::<Application, WasmApplication>(app) };
        std::mem::forget(wasm_app.0.clone());
        unsafe { std::mem::transmute::<WasmApplication, Application>(wasm_app) }
    };

    app.run(|cx: &mut App| {
        cx.activate(true);
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|_cx| HelloWorld)
        })
        .expect("open window");
    });

    log::info!("HelloWorld window opened");
}

struct HelloWorld;

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x1e1e2e))
            .text_xl()
            .text_color(rgb(0xffffff))
            .child("Hello from Cditor WASM!")
    }
}

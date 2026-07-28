async function init() {
  const loadingEl = document.getElementById("loading");
  const appEl = document.getElementById("app");

  try {
    console.log("[Cditor] Importing WASM...");
    const wasm = await import("./wasm/cditor_web.js");
    console.log("[Cditor] Instantiating module...");
    await wasm.default();
    console.log("[Cditor] Starting editor...");
    await wasm.run();
    console.log("[Cditor] Done! Hiding loading...");
    if (appEl) {
      appEl.remove();
    }
  } catch (error) {
    console.error("[Cditor] Failed:", error);
    if (loadingEl) {
      loadingEl.innerHTML = `<div class="error"><h2>加载失败</h2><p>${error.message || error}</p></div>`;
    }
  }
}

init();

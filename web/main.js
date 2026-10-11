// Loads the app (built by tools/build_web.sh). Everything runs in this
// page; files you drop are read here and never sent anywhere.
import init from "./lightable.js";

const status = document.getElementById("status");

const params = new URLSearchParams(location.search);
// Recording frames for the demo: 16:9 or 9:16.
const format = params.get("format");
if (format === "landscape" || format === "vertical") document.body.classList.add(`format-${format}`);

// The preview shows the default example; hide it for anything else.
const example = params.get("example");
if (example && example !== "bracket") document.getElementById("preview-row")?.remove();

const mb = (n) => (n / 1e6).toFixed(1);

// Counts the bytes of the app as they arrive, without holding them back:
// the counted response still goes to WebAssembly.instantiateStreaming,
// which compiles while it downloads. The bytes counted are the unpacked
// ones, so the total is the unpacked size written in at build time (the
// Content-Length of a compressed response is the packed size).
function withProgress(response) {
  if (!response.ok || !response.body) return response;
  const packed = response.headers.get("Content-Encoding");
  const total = Number(status.dataset.wasmBytes) || (!packed && Number(response.headers.get("Content-Length"))) || 0;
  let loaded = 0;
  const show = () => {
    status.textContent = total
      ? `Loading Lightable… ${Math.min(100, Math.floor((100 * loaded) / total))} %`
      : `Loading Lightable… ${mb(loaded)} MB`;
  };
  const counted = response.body.pipeThrough(new TransformStream({
    transform(chunk, out) {
      loaded += chunk.byteLength;
      show();
      out.enqueue(chunk);
    },
    flush() {
      status.textContent = "Starting…";
    },
  }));
  return new Response(counted, { status: response.status, statusText: response.statusText, headers: response.headers });
}

init({ module_or_path: fetch(new URL("lightable_bg.wasm", import.meta.url)).then(withProgress) }).then(
  () => document.getElementById("loading")?.remove(),
  (e) => {
    document.getElementById("preview-row")?.remove();
    const webgl = /webgl/i.test(String(e));
    status.textContent = webgl
      ? "Lightable needs WebGL2, which this browser has turned off or does not support. " +
        "Try a recent Chrome, Firefox, Safari or Edge, or turn on hardware acceleration."
      : "Lightable could not start in this browser. Reloading the page may help.";
    const detail = document.createElement("div");
    detail.className = "detail";
    detail.textContent = "Details: " + e;
    status.after(detail);
  },
);

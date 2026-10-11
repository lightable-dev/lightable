// Browser test harness (development only; not part of the app).
// Served from the repository root:
//   python3 tools/web_test_server.py   then open
//   http://localhost:8765/tests/web/harness.html?a=/tests/corpus/X.dxf&b=/tests/corpus/Y.dxf
// It starts the real app, fetches the files named by ?a= and ?b=, drops
// them on the canvas like a user's drag and drop, optionally clicks at
// ?clicks=x,y;x,y (CSS pixels), and then reports every network request
// the page made after the drop plus any blocked by the Content Security
// Policy. The result is shown at the bottom and in the title.
import init from "/target/web/lightable.js";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const params = new URLSearchParams(location.search);
const out = document.getElementById("report");
const violations = [];
document.addEventListener("securitypolicyviolation", (e) =>
  violations.push(`${e.violatedDirective} ${e.blockedURI}`));
const errors = [];
window.addEventListener("error", (e) => errors.push(String(e.message)));
window.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));

const canvas = document.getElementById("cad_canvas");

function pointer(type, x, y) {
  canvas.dispatchEvent(new PointerEvent(type, {
    clientX: x, clientY: y, button: 0, buttons: type === "pointerup" ? 0 : 1,
    pointerId: 1, pointerType: "mouse", isPrimary: true, bubbles: true, cancelable: true,
  }));
  const mouse = { pointerdown: "mousedown", pointerup: "mouseup", pointermove: "mousemove" }[type];
  canvas.dispatchEvent(new MouseEvent(mouse, {
    clientX: x, clientY: y, button: 0, buttons: type === "pointerup" ? 0 : 1, bubbles: true, cancelable: true,
  }));
}

async function run() {
  const t0 = performance.now();
  await init();
  await sleep(1500);
  const files = [];
  for (const key of ["a", "b"]) {
    const url = params.get(key);
    if (!url) continue;
    const r = await fetch(url);
    files.push(new File([await r.arrayBuffer()], url.split("/").pop()));
  }
  const before = performance.getEntriesByType("resource").length;
  if (files.length) {
    const dt = new DataTransfer();
    for (const f of files) dt.items.add(f);
    for (const type of ["dragenter", "dragover", "drop"]) {
      canvas.dispatchEvent(new DragEvent(type, { dataTransfer: dt, bubbles: true, cancelable: true }));
    }
  }
  await sleep(Number(params.get("wait") || 6000));
  for (const xy of (params.get("clicks") || "").split(";").filter(Boolean)) {
    const [x, y] = xy.split(",").map(Number);
    pointer("pointermove", x, y);
    await sleep(150);
    pointer("pointerdown", x, y);
    await sleep(80);
    pointer("pointerup", x, y);
    await sleep(1500);
  }
  await sleep(1000);
  const all = performance.getEntriesByType("resource");
  const after = all.slice(before).map((e) => e.name);
  const ms = Math.round(performance.now() - t0);
  out.textContent =
    `${navigator.userAgent} | files: ${files.map((f) => `${f.name} (${f.size} B)`).join(", ") || "none"}` +
    ` | requests after the drop: ${after.length} ${after.join(" ")}` +
    ` | CSP violations: ${violations.length} ${violations.join(" ")}` +
    ` | errors: ${errors.length} ${errors.join(" ")}` +
    ` | every request: ${all.map((e) => e.name.replace(location.origin, "")).join(" ")} | ${ms} ms`;
  document.title = `DONE after=${after.length} csp=${violations.length} errors=${errors.length}`;
  // Hand the result to the test server's log (browsers whose screen we do
  // not read). This is the harness's own request, made after the
  // measurement above, at a path the server labels as a harness result.
  if (params.has("post")) {
    await fetch("/__harness-result?" + new URLSearchParams({ r: out.textContent }));
  }
}

run().catch((e) => {
  out.textContent = "harness error: " + e;
  document.title = "FAILED";
});

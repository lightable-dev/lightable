// The demo video pipeline: stills with automatic checks, draft videos and
// final videos, each published to the private review page (out/review/).
//
//   node tools/demo.mjs stills [--modes normal,narration] [--note "..."]
//   node tools/demo.mjs draft  [--modes normal,narration] [--note "..."]
//   node tools/demo.mjs final  [--modes normal,narration] [--note "..."]
//   node tools/demo.mjs approve --mode normal|narration
//
// `approve` copies the current FINAL videos of that mode (never drafts),
// their captions and two posters into web/demo/, where tools/build_web.sh
// picks them up for lightable.dev/demo/. Run it only for a final you have
// approved.
//
// Options: --formats landscape,vertical (default both), --base URL of the
// web build (default http://localhost:8765/target/web/index.html, served
// by tools/web_test_server.py), --no-review (do not publish).
//
// The app plays the demo on a frame clock the tool sets (`?demo&record`):
// demo time = frame / 30, and every zoom starts at its scheduled time. The
// tool performs the person's gestures (the plan's `__lightableDemoGestures`)
// as browser input events: the example files dragged and dropped, mouse
// moves and clicks, ctrl + wheel to pinch, wheel steps to scroll the list.
// Frames a gesture needs are always drawn in order, so any frame looks the
// same however it is reached. Stills jump
// straight to the moments that matter (each step settled, and mid-zoom), so
// a still set takes seconds. Videos are cut into segments at the demo's
// steps. A segment is cached under a hash of its timeline and settings (the
// steps up to it, format, quality); before it is reused, the current build
// draws three probe frames (start, middle, end) and they must match the
// cached ones exactly, so only steps that look different are recorded
// again. Segments are joined with ffmpeg. While the picture holds still (no
// animation, nothing changed) a frame is drawn once and repeated.
//
// Needs Chrome, ffmpeg (h264_videotoolbox for drafts if available, libx264
// for finals) and `target/release/lightable` (for the subtitles).
// Development only.

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync, cpSync, existsSync, linkSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync,
} from "node:fs";
import { cpus, tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";

// Node 22 or later: the browser is driven over the built-in WebSocket.
if (typeof WebSocket === "undefined") {
  console.error(`tools/demo.mjs needs Node 22 or later (this is ${process.version}).`);
  process.exit(2);
}
const ROOT = resolve(new URL("..", import.meta.url).pathname);
// The machine recordings are made on, named next to any time measured in
// them (the app's timing line): "a MacBook Air (2-core Intel i3), while
// recording".
const MACHINE = (() => {
  const sysctl = (k) => spawnSync("sysctl", ["-n", k], { encoding: "utf8" }).stdout?.trim() || "";
  const brand = sysctl("machdep.cpu.brand_string") || cpus()[0]?.model || "";
  const chip = brand.match(/Core\(TM\) (i\d-\w+)/)?.[1] ? `Intel ${brand.match(/Core\(TM\) (i\d)-/)[1]}` : brand.replace(/\(R\)|\(TM\)|CPU.*$/g, "").trim();
  const cores = Number(sysctl("hw.physicalcpu")) || cpus().length;
  const model = sysctl("hw.model").replace(/\d+,\d+$/, "").replace(/^MacBook(Air|Pro)$/, "MacBook $1");
  return `${model ? `a ${model}` : "this computer"} (${cores}-core ${chip}), while recording`;
})();
const OUT = join(ROOT, "out", "demo");
const REVIEW = join(ROOT, "out", "review");
const ASSETS = join(ROOT, "crates", "lightable", "assets");
const TOOL_VERSION = 8;
// The demo page's feature videos (`--modes find,explain,...`; `--modes
// features` for all five).
const FEATURES = ["unclouded", "tiny", "resave", "blocks", "big"];
const END_CARD = "Free. Open source. Your files are read in your browser, never uploaded. lightable.dev";
const CLOCK = 30; // the app's frame clock
// The use-case videos are recorded inside the brand stage (`--plain`: the
// app alone, as before).
const isStaged = (mode) => FEATURES.includes(mode) && !process.argv.includes("--plain");
// The stage's app window width (CSS px, before the camera), as in
// tools/stage/stage.js.
const FRAME_APP_WIDTH = { landscape: 912, vertical: 524 };

const args = process.argv.slice(2);
const command = args[0];
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const formats = opt("formats", "landscape,vertical").split(",");
const modes = opt("modes", "normal,narration").split(",").flatMap((m) => (m === "features" ? FEATURES : [m]));
// Output names: demo-landscape.mp4, demo-landscape-narration.mp4,
// demo-landscape-find.mp4, ...
const suffix = (mode) => (mode === "normal" ? "" : `-${mode}`);
const captionsFile = (mode) => (mode === "normal" ? "demo.vtt" : mode === "narration" ? "demo-narration.vtt" : `feature-${mode}.vtt`);
const base = opt("base", "http://localhost:8765/target/web/index.html");
const note = opt("note", "");
const review = !args.includes("--no-review");
// The demo page's videos, posters and captions into `dir`, from the
// videos `video(feature, format)` names. Posters: a still from each video.
const POSTER_AT = { unclouded: 13, tiny: 10, resave: 9, blocks: 7, big: 11 };
function pageFiles(dir, video) {
  for (const m of FEATURES) {
    for (const format of ["landscape", "vertical"]) {
      const src = video(m, format);
      if (!existsSync(src)) throw new Error(`no video ${src}`);
      copyFileSync(src, join(dir, `${m}-${format}.mp4`));
      spawnSync("ffmpeg", ["-v", "error", "-y", "-ss", String(POSTER_AT[m]), "-i", src, "-frames:v", "1", "-q:v", "3",
        join(dir, `${m}-${format}.jpg`)]);
    }
    // (No subtitle track: the captions are in the picture, so only one set
    // ever shows.)
  }
}
if (command === "approve") {
  const mode = opt("mode", "");
  if (!["normal", "narration", "features"].includes(mode)) {
    console.error("usage: node tools/demo.mjs approve --mode normal|narration|features");
    process.exit(2);
  }
  const web = join(ROOT, "web", "demo");
  if (mode === "features") {
    // The five feature videos of the demo page: finals only.
    pageFiles(web, (m, format) => join(OUT, `demo-${format}-${m}.mp4`));
    console.log("Approved feature finals copied to web/demo/; tools/build_web.sh will publish the page at /demo/.");
    process.exit(0);
  }
  const suffix = mode === "narration" ? "-narration" : "";
  for (const format of ["landscape", "vertical"]) {
    const src = join(OUT, `demo-${format}${suffix}.mp4`);
    if (!existsSync(src)) throw new Error(`no final video ${src}: run \`node tools/demo.mjs final\` first`);
    copyFileSync(src, join(web, `demo-${format}.mp4`));
    // Poster: the overlay and the change list, a still from the video itself.
    spawnSync("ffmpeg", ["-v", "error", "-y", "-ss", mode === "narration" ? "16" : "10", "-i", src, "-frames:v", "1", "-q:v", "3",
      join(web, `poster-${format}.jpg`)]);
  }
  copyFileSync(join(OUT, mode === "narration" ? "demo-narration.vtt" : "demo.vtt"), join(web, "demo.vtt"));
  console.log(`Approved ${mode} final copied to web/demo/ (the whole demo; the page shows the feature videos).`);
  process.exit(0);
}
if (!["stills", "cut", "draft", "final", "page"].includes(command)) {
  console.error("usage: node tools/demo.mjs stills|cut|draft|final|page [--modes normal,narration|features] [--formats landscape,vertical] [--workers N] [--note TEXT]");
  process.exit(2);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// Time spent per phase (printed with --profile).
const PROFILE = {};
const timed = async (phase, work) => {
  const t0 = Date.now();
  try {
    return await work();
  } finally {
    PROFILE[phase] = (PROFILE[phase] ?? 0) + Date.now() - t0;
  }
};
const sha = (data) => createHash("sha256").update(data).digest("hex");
const FRAME = { landscape: [960, 540], vertical: [540, 960] };
const hasVideoToolbox = spawnSync("ffmpeg", ["-hide_banner", "-encoders"]).stdout.toString().includes("h264_videotoolbox");
const QUALITY = {
  stills: { dpr: 2 },
  // Like draft, but every step cuts in without its zoom: content review in
  // seconds.
  // Mostly held frames in short segments: x264 "ultrafast" starts quicker
  // than a VideoToolbox session per segment.
  // Drafts render at 2x like finals (on a high-density screen Chrome
  // reports 2x device pixels to the app even when 1x is emulated, which
  // laid 1x pages out at half scale) and are captured scaled to half.
  cut: {
    dpr: 2, shrink: 0.5, fps: 15, cut: true,
    encode: ["-c:v", "libx264", "-preset", "ultrafast", "-crf", "23", "-pix_fmt", "yuv420p"],
  },
  draft: {
    dpr: 2, shrink: 0.5, fps: 15,
    encode: hasVideoToolbox
      ? ["-c:v", "h264_videotoolbox", "-b:v", "3M", "-profile:v", "high", "-vf", "format=yuv420p"]
      : ["-c:v", "libx264", "-preset", "veryfast", "-crf", "23", "-pix_fmt", "yuv420p"],
  },
  final: {
    dpr: 2, fps: 30,
    encode: ["-c:v", "libx264", "-preset", "slow", "-tune", "animation", "-crf", "16", "-pix_fmt", "yuv420p"],
  },
};

// --- A browser page playing the demo on the recorder's clock -------------

// Browsers working at once (`--workers N`): half the physical cores, 1 to
// 4. Each browser plus its share of x264 keeps about two cores busy; on a
// 2-core Mac two workers were slower than one (measured).
function physicalCores() {
  const mac = spawnSync("sysctl", ["-n", "hw.physicalcpu"]).stdout?.toString().trim();
  if (mac && Number(mac) > 0) return Number(mac);
  const linux = spawnSync("sh", ["-c", "lscpu -p=core 2>/dev/null | grep -v '^#' | sort -u | wc -l"]).stdout?.toString().trim();
  if (linux && Number(linux) > 0) return Number(linux);
  return Math.max(1, Math.floor(cpus().length / 2));
}
// Stills and cut drafts mostly wait for pages to load: at least two there.
const WORKERS = Number(opt("workers", 0))
  || Math.min(4, Math.max(["stills", "cut"].includes(command) ? 2 : 1, Math.floor(physicalCores() / 2)));
const CHROME = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
let nextPort = 9350 + Math.floor(Math.random() * 300);

// One Chrome for the whole run (launching one with a fresh profile takes
// about 8 s); each page gets its own window, so none is a hidden tab.
let browser = null;
function sharedBrowser() {
  browser ??= (async () => {
    const port = nextPort++;
    const proc = spawn(CHROME, [
      `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "demo-"))}`,
      "--no-first-run", "--no-default-browser-check", "--window-size=1400,1000",
      "--disable-background-timer-throttling", "--disable-renderer-backgrounding",
      "--disable-backgrounding-occluded-windows", "about:blank",
    ], { stdio: "ignore" });
    // Never left running, even when the run fails.
    process.on("exit", () => proc.kill());
    let version;
    for (let i = 0; i < 120 && !version; i++) {
      try {
        version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
      } catch {}
      if (!version) await sleep(250);
    }
    const ws = new WebSocket(version.webSocketDebuggerUrl);
    await new Promise((r) => ws.addEventListener("open", r));
    let id = 0;
    const pending = new Map();
    // Downloads (the demo's exported report), by the page that made them.
    const downloads = new Map();
    ws.addEventListener("message", (m) => {
      const d = JSON.parse(m.data);
      if (pending.has(d.id)) {
        pending.get(d.id)(d.result ?? { error: d.error });
        pending.delete(d.id);
      }
      if (d.method === "Browser.downloadWillBegin") {
        downloads.set(d.params.guid, { frame: d.params.frameId, name: d.params.suggestedFilename, state: "started" });
      }
      if (d.method === "Browser.downloadProgress" && downloads.has(d.params.guid)) {
        downloads.get(d.params.guid).state = d.params.state;
      }
    });
    const send = (method, params = {}) => new Promise((r) => {
      const n = ++id;
      pending.set(n, r);
      ws.send(JSON.stringify({ id: n, method, params }));
    });
    // Saved under their download id in one folder for the run.
    const folder = mkdtempSync(join(tmpdir(), "demo-export-"));
    await send("Browser.setDownloadBehavior", { behavior: "allowAndName", downloadPath: folder, eventsEnabled: true });
    return { port, proc, send, downloads, folder };
  })();
  return browser;
}

async function openDemo(format, mode, dpr, cut = false) {
  const tOpen = Date.now();
  const shared = await sharedBrowser();
  const { port } = shared;
  const { targetId } = await shared.send("Target.createTarget", { url: "about:blank", newWindow: true });
  const ws = new WebSocket(`ws://127.0.0.1:${port}/devtools/page/${targetId}`);
  await new Promise((r) => ws.addEventListener("open", r));
  const tLaunched = Date.now();
  let id = 0;
  const pending = new Map();
  const requests = [];
  const errors = [];
  ws.addEventListener("message", (m) => {
    const d = JSON.parse(m.data);
    if (pending.has(d.id)) {
      pending.get(d.id)(d.result ?? { error: d.error });
      pending.delete(d.id);
    }
    if (d.method === "Network.requestWillBeSent") {
      const r = d.params.request;
      requests.push({ method: r.method, url: r.url, body: r.postData ? r.postData.length : 0 });
    }
    if (d.method === "Runtime.exceptionThrown") errors.push(d.params.exceptionDetails.text);
    if (d.method === "Runtime.consoleAPICalled" && d.params.type === "error") {
      errors.push(d.params.args.map((a) => a.value ?? a.description).join(" "));
    }
  });
  const send = (method, params = {}) => new Promise((r) => {
    const n = ++id;
    pending.set(n, r);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
  const evaluate = async (expression) =>
    (await send("Runtime.evaluate", { expression, returnByValue: true })).result?.value;
  const nudge = async () => {
    // On the stage: the same Shift tap, given to the app's canvas in its
    // frame (browser key input goes to the stage page).
    if (isStaged(mode)) {
      await evaluate(`(() => { const a = __app(); const c = a.document.getElementById("cad_canvas") ?? a.document;
        for (const type of ["keydown", "keyup"]) c.dispatchEvent(new a.KeyboardEvent(type, { key: "Shift", code: "ShiftLeft", bubbles: true }));
        return true; })()`);
      return;
    }
    // A Shift tap (no visible effect) makes the app draw a frame.
    await send("Input.dispatchKeyEvent", { type: "rawKeyDown", key: "Shift", code: "ShiftLeft", windowsVirtualKeyCode: 16 });
    await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Shift", code: "ShiftLeft", windowsVirtualKeyCode: 16 });
  };
  await send("Network.enable");
  await send("Runtime.enable");
  await send("Page.enable");
  await send("Page.bringToFront");
  const [w, h] = FRAME[format];
  await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: dpr, mobile: false });
  // (On the stage the app fills its window: no fixed-shape frame.)
  const query = `demo&record${isStaged(mode) ? "" : `&format=${format}`}${mode === "narration" ? "&narration" : ""}${FEATURES.includes(mode) ? `&feature=${mode}` : ""}${cut ? "&cut" : ""}&machine=${encodeURIComponent(MACHINE)}`;
  // The use-case videos play inside the brand stage (tools/stage/): the
  // real app in a window mockup, the motion graphics around it.
  const staged = isStaged(mode);
  const url = staged
    ? `${new URL(base).origin}/tools/stage/index.html?format=${format}&app=${encodeURIComponent(query)}`
    : `${base}?${query}`;
  // The app's own globals: in its frame on the stage.
  const ev = (expression) => evaluate(staged ? expression.replaceAll("window.__lightable", "__app().__lightable") : expression);
  await send("Page.navigate", { url });
  for (let i = 0; i < 600; i++) {
    const loaded = staged
      // (The frame starts as a blank page, already "complete": wait for the
      // app's own canvas.)
      ? "(() => { try { const d = __app().document; return d.readyState === 'complete' && !!d.getElementById('cad_canvas') && !d.getElementById('loading') && __stage.ready(); } catch (e) { return false; } })()"
      : "!document.getElementById('loading')";
    if (await evaluate(loaded)) break;
    await sleep(100);
  }
  // Keys (the nudge) go to the app's frame.
  if (staged) await evaluate("document.getElementById('app').focus(); __app().focus(); true");
  const tLoaded = Date.now();
  // A window on a high-density screen can report its own pixel density
  // before the emulated one, and the app keeps the canvas size it saw: a
  // resize makes it measure again (1x pages were laid out at half scale).
  const remeasure = async () => {
    await send("Emulation.setDeviceMetricsOverride", { width: w + 2, height: h, deviceScaleFactor: dpr, mobile: false });
    await sleep(200);
    await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: dpr, mobile: false });
    await sleep(200);
  };
  await remeasure();
  // Browser (CSS) pixels across the page: input events are given in them.
  const cssWidth = await evaluate(staged ? "__app().innerWidth" : "innerWidth");
  await ev("window.__lightableFrame = 0");
  let length;
  for (let i = 0; i < 600 && !length; i++) {
    await nudge();
    length = await ev("window.__lightableDemoLength");
    if (!length) await sleep(100);
  }
  if (!length) throw new Error(`the demo did not start: ${url} ${errors.join(" | ")}`);
  // The app's width in points times its zoom must be the page's width, or
  // the pointer would land beside what it aims at.
  for (let i = 0; ; i++) {
    for (let k = 0; k < 3; k++) {
      await nudge();
      await sleep(60);
    }
    const now = JSON.parse(await ev("window.__lightableFrameInfo"));
    const ratio = cssWidth / (now.screen[2] - now.screen[0]);
    if (Math.abs(ratio - now.zoom) < 0.01) break;
    if (i >= 10) throw new Error(`the page is drawn at the wrong scale (${ratio} CSS pixels a point, zoom ${now.zoom})`);
    await remeasure();
  }
  const shots = JSON.parse(await ev("window.__lightableDemoShots"));
  const gestures = JSON.parse(await ev("window.__lightableDemoGestures"));
  // The files each drop drops, on disk: copies of the example's files (the
  // same date, so only their names could tell the order) or written bytes.
  const fileDir = mkdtempSync(join(tmpdir(), "demo-files-"));
  const fileSets = {};
  for (const [set, list] of Object.entries(JSON.parse(await ev("window.__lightableDemoFiles")))) {
    fileSets[set] = list.map((f) => {
      const path = join(fileDir, f.name);
      // (The large layout comes from out/demo/usecases, made by
      // tools/gen_usecases.py.)
      if (f.copy) copyFileSync(f.copy.startsWith("usecases/") ? join(OUT, f.copy) : join(ASSETS, f.copy), path);
      else writeFileSync(path, Buffer.from(f.hex, "hex"));
      utimesSync(path, new Date("2026-01-05T09:00:00Z"), new Date("2026-01-05T09:00:00Z"));
      return path;
    });
  }
  // The report the demo exports is really saved (the browser reports it).
  const exports = () => [...shared.downloads.entries()]
    .filter(([, d]) => d.frame === targetId && d.state === "completed")
    .map(([guid, d]) => ({ name: d.name, file: join(shared.folder, guid) }));
  PROFILE.launch = (PROFILE.launch ?? 0) + (tLaunched - tOpen);
  PROFILE.load = (PROFILE.load ?? 0) + (tLoaded - tLaunched);
  PROFILE.start = (PROFILE.start ?? 0) + (Date.now() - tLoaded);
  let last = -1;
  const draws = async () => (await ev("window.__lightableDraws")) ?? 0;
  const info = async () => JSON.parse(await ev("window.__lightableFrameInfo"));
  // Waits for the app to draw after `before` draws; nudges if it does not.
  const drawn = async (before) => {
    for (let i = 0; i < 4000; i++) {
      // Only a fallback: an extra nudge can mean an extra draw, so wait
      // long enough for a busy machine.
      if (i > 0 && i % 600 === 0) await nudge();
      if ((await draws()) > before) return;
      await sleep(5);
    }
    const state = await ev("JSON.stringify({frame: window.__lightableFrame, rendered: window.__lightableRendered, draws: window.__lightableDraws})");
    writeFileSync(join(tmpdir(), "demo-stall.png"), Buffer.from((await send("Page.captureScreenshot", { format: "png" })).data, "base64"));
    throw new Error(`the app stopped drawing (${state}): ${errors.join(" | ")}`);
  };
  // --- The person: pointer, clicks, drop and pinch, as browser input -----
  // Frames a gesture needs drawn in order (the pointer's path, the click,
  // each step of the pinch, and a few frames after): never skipped.
  const busyFrames = (step) => gestures.map((g) => [
    Math.floor((g.from * CLOCK) / step) * step - step,
    // (A second of slack: a click can come late after the list scrolls.)
    Math.ceil(((g.until + 1) * CLOCK) / step) * step + 3 * step,
  ]);
  const driver = { next: 0, path: null, pos: null, zoomed: 0, dragging: false, presses: [], hover: null, drop: null, dropped: null, dropAnswered: null };
  let latest = null; // the last frame's info
  // On the stage: where the app's frame is this frame (CSS px, after the
  // camera), and what was found dishonest.
  let M = null;
  const honesty = [];
  const stageTray = staged ? JSON.parse(await evaluate("JSON.stringify(__stage.tray())")) : null;
  // The input events for frame f (CSS pixels), from the plan's gestures and
  // where their targets were on the last frame.
  function eventsFor(f, cut) {
    const t = f / CLOCK;
    const events = [];
    for (;;) {
      const g = gestures[driver.next];
      if (!g || t < (cut && !staged ? g.at : g.from)) return events;
      // CSS pixels per app point (the app's zoom; measured, as 1x pages
      // scale differently).
      const z = (staged ? M.w : cssWidth) / (latest.screen[2] - latest.screen[0]);
      const P = (x, y) => (staged ? [M.x + x * z, M.y + y * z] : [x * z, y * z]);
      // (On the stage, cut drafts keep the person's motion: only the app's
      // zooms are cut.)
      const instant = cut && !staged;
      const click = !g.target.startsWith("drop") && g.target !== "pinch";
      // A row outside the list's view: the pointer goes to the list and
      // scrolls it (small wheel steps, which egui applies at once) first.
      const row = latest.targets[g.target.startsWith("drop") ? "drop" : g.target];
      const scrolls = g.target.startsWith("row:") || g.target === "not_compared";
      const list = latest.targets.list;
      // In view (a point of slack), or no longer moving as the list scrolls.
      // (Three frames in a row: one scroll step can land a frame late.)
      if (driver.scroll && row) {
        driver.scroll.still = driver.scroll.last === row[1] ? (driver.scroll.still ?? 0) + 1 : 0;
      }
      const stuck = (driver.scroll?.still ?? 0) >= 3;
      const inside = (r, o) => stuck || (r[1] >= o[1] - 3 && r[3] <= o[3] + 3 && r[3] <= latest.screen[3]);
      driver.hover = null;
      if (!driver.path && !instant && scrolls && row && list && !inside(row, list)) {
        const centre = P((list[0] + list[2]) / 2, (list[1] + list[3]) / 2);
        driver.scroll ??= { start: t, from: driver.pos ?? centre };
        const k = Math.min(1, (t - driver.scroll.start) / 0.3);
        const e = k * k * (3 - 2 * k);
        const pos = [driver.scroll.from[0] + (centre[0] - driver.scroll.from[0]) * e, driver.scroll.from[1] + (centre[1] - driver.scroll.from[1]) * e];
        events.push(["Input.dispatchMouseEvent", { type: "mouseMoved", x: pos[0], y: pos[1], button: "none" }]);
        driver.pos = pos;
        if (k >= 1) {
          driver.scroll.last = row[1];
          const down = row[3] > list[3] ? 7 : -7;
          // Faster when the row is far off (done in about six frames), so
          // the pointer still has time to move to it and pause.
          const far = row[3] > list[3] ? row[3] - list[3] : list[1] - row[1];
          const steps = Math.min(10, Math.max(3, Math.ceil(far / 7 / 6)));
          for (let n = 0; n < steps; n++) {
            events.push(["Input.dispatchMouseEvent", { type: "mouseWheel", x: pos[0], y: pos[1], deltaX: 0, deltaY: down }]);
          }
        }
        return events;
      }
      if (instant && scrolls && row && list && !inside(row, list)) {
        // Cut drafts: scroll at once (still for real), then click.
        const centre = P((list[0] + list[2]) / 2, (list[1] + list[3]) / 2);
        driver.scroll ??= { start: t, from: centre };
        driver.scroll.last = row[1];
        // The wheel scrolls what the pointer is over.
        events.push(["Input.dispatchMouseEvent", { type: "mouseMoved", x: centre[0], y: centre[1], button: "none" }]);
        const down = row[3] > list[3] ? 7 : -7;
        for (let n = 0; n < 3; n++) {
          events.push(["Input.dispatchMouseEvent", { type: "mouseWheel", x: centre[0], y: centre[1], deltaX: 0, deltaY: down }]);
        }
        driver.pos = centre;
        return events;
      }
      driver.scroll = null;
      if (!driver.path) {
        let to;
        let factor = 1;
        if (g.target === "pinch") {
          if (!latest.pinch) throw new Error(`frame ${f}: no revision cloud to pinch`);
          to = P(latest.pinch[0], latest.pinch[1]);
          factor = latest.pinch[2];
        } else {
          const r = latest.targets[g.target.startsWith("drop") ? "drop" : g.target];
          if (r && (r[3] > latest.screen[3] + 1 || r[2] > latest.screen[2] + 1)) throw new Error(`frame ${f}: "${g.target}" is outside the frame (${r})`);
          if (!r) throw new Error(`frame ${f}: "${g.target}" is not on screen (on screen: ${Object.keys(latest.targets).join(", ")}; list ${latest.list}, dropped ${latest.dropped})`);
          to = P((r[0] + r[2]) / 2, (r[1] + r[3]) / 2);
        }
        // The first gesture comes in from the right-hand edge.
        const [w, h] = staged ? FRAME[format] : [latest.screen[2] * z, latest.screen[3] * z];
        // Clicks: the pointer takes at most 0.8 s to get there, and at
        // least 0.45 s plus the 0.3 s pause, even when that makes the click
        // late (a list that had to scroll first): never a jump.
        const at = click && !instant ? Math.max(g.at, t + 0.75) : g.at;
        driver.path = { from: driver.pos ?? [w - 2, h * 0.6], to, factor, at, start: click ? Math.max(t, at - 0.8) : g.from };
        // On the stage a drop has two legs: to the file cards (the pointer
        // picks them up: the real drag starts there), then to the window.
        if (staged && g.target.startsWith("drop")) {
          driver.path.tray = stageTray;
          driver.path.pickup = g.from + 0.85;
          driver.drop = { from: g.from, pickup: driver.path.pickup, at: g.at, point: to, names: (fileSets[g.target] ?? []).map((f) => basename(f)) };
        }
      }
      const { from, factor, start, at } = driver.path;
      // Aimed at where the control is now (a scrolled list can still be
      // settling).
      const live = click && latest.targets[g.target];
      if (live && !instant) driver.path.to = P((live[0] + live[2]) / 2, (live[1] + live[3]) / 2);
      const to = driver.path.to;
      const drag = g.target.startsWith("drop");
      const data = { items: [], files: fileSets[g.target] ?? [], dragOperationsMask: 1 };
      if (t < at) {
        // Eased, arriving 0.3 s before the click: a short pause on the
        // control (its hover look shows) before pressing.
        let k = Math.min(1, Math.max(0, (t - start) / Math.max(1e-3, at - 0.3 - start)));
        k = k * k * (3 - 2 * k);
        let pos = [from[0] + (to[0] - from[0]) * k, from[1] + (to[1] - from[1]) * k];
        const smooth = (v) => { const c = Math.min(1, Math.max(0, v)); return c * c * (3 - 2 * c); };
        if (drag && driver.path.tray) {
          const { tray, pickup } = driver.path;
          if (t < pickup) {
            // To the cards, a short pause on them.
            const k1 = smooth((t - (start + 0.1)) / Math.max(1e-3, pickup - 0.15 - (start + 0.1)));
            pos = [from[0] + (tray[0] - from[0]) * k1, from[1] + (tray[1] - from[1]) * k1];
            events.push(["Input.dispatchMouseEvent", { type: "mouseMoved", x: pos[0], y: pos[1], button: "none" }]);
          } else {
            // Carried to the window: the real drag.
            const k2 = smooth((t - pickup) / Math.max(1e-3, at - 0.3 - pickup));
            pos = [tray[0] + (to[0] - tray[0]) * k2, tray[1] + (to[1] - tray[1]) * k2];
            events.push(["Input.dispatchDragEvent", { type: driver.dragging ? "dragOver" : "dragEnter", x: pos[0], y: pos[1], data }]);
            driver.dragging = true;
          }
        } else if (drag) {
          events.push(["Input.dispatchDragEvent", { type: driver.dragging ? "dragOver" : "dragEnter", x: pos[0], y: pos[1], data }]);
          driver.dragging = true;
        } else {
          // Every frame, even while pausing on the control: another page
          // brought to the front (a parallel worker) can make this one
          // forget where the mouse is.
          events.push(["Input.dispatchMouseEvent", { type: "mouseMoved", x: pos[0], y: pos[1], button: "none" }]);
          // Paused on the control it is about to press: its hover look.
          if (click && t >= at - 0.3) driver.hover = g.target;
        }
        driver.pos = pos;
        return events;
      }
      const [x, y] = to;
      if (drag) {
        if (!driver.dragging) events.push(["Input.dispatchDragEvent", { type: "dragEnter", x, y, data }]);
        events.push(["Input.dispatchDragEvent", { type: "dragOver", x, y, data }]);
        events.push(["Input.dispatchDragEvent", { type: "drop", x, y, data }]);
        driver.dragging = false;
        driver.dropped = t;
      } else {
        if (!driver.pos || x !== driver.pos[0] || y !== driver.pos[1]) {
          events.push(["Input.dispatchMouseEvent", { type: "mouseMoved", x, y, button: "none" }]);
        }
        if (g.target === "pinch") {
          // Ctrl + wheel, as a trackpad pinch sends it: the browser's
          // factor is exp(-deltaY / 100).
          const p = cut || g.until <= g.at ? 1 : Math.min(1, (t - g.at) / (g.until - g.at));
          const step = factor ** (p - driver.zoomed);
          driver.zoomed = p;
          events.push(["Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX: 0, deltaY: -100 * Math.log(step), modifiers: 2 }]);
          if (p < 1) {
            driver.pos = to;
            return events;
          }
          driver.zoomed = 0;
        } else {
          events.push(["Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 }]);
          driver.presses.push({ t, x, y, target: g.target });
          events.push(["Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1 }]);
        }
      }
      driver.pos = to;
      driver.path = null;
      driver.next++;
    }
  }
  // Draws frame f: its input events first (each drawn), or a nudge; then
  // again while the app is busy (files being read, the report captured).
  async function draw(f, cut) {
    await ev(`window.__lightableFrame = ${f}`);
    // The stage's camera for this frame first: where input lands.
    if (staged) M = JSON.parse(await evaluate(`JSON.stringify(__stage.camera(${f}))`));
    const events = latest ? eventsFor(f, cut) : [];
    if (events.length === 0) {
      const before = await draws();
      await nudge();
      await drawn(before);
    }
    for (const [method, params] of events) {
      const before = await draws();
      await send(method, params);
      await drawn(before);
    }
    for (let i = 0; i < 6000; i++) {
      if ((await ev("window.__lightableRendered")) === f) break;
      await sleep(5);
    }
    let now = await info();
    if (process.env.DEMO_DEBUG) console.error(f, JSON.stringify(events.map(([m, p]) => [p.type, Math.round(p.x), Math.round(p.y)])), now.pointer);
    for (let i = 0; now.busy; i++) {
      if (i > 400) throw new Error(`frame ${f}: the app stayed busy: ${errors.join(" | ")}`);
      await sleep(20);
      const before = await draws();
      await nudge();
      await drawn(before);
      now = await info();
    }
    latest = now;
    last = f;
    if (!staged) return now;
    // The decorations around the frame, from what really happened.
    const t = f / CLOCK;
    const input = {
      pointer: driver.pos, hover: !!driver.hover, drop: driver.drop,
      down: driver.presses.some((p) => t - p.t >= 0 && t - p.t < 0.12),
      presses: driver.presses.map(({ t: pt, x, y, target }) => ({ t: pt, x, y, target })),
    };
    const report = JSON.parse(await evaluate(`__stage.paint(${f}, ${JSON.stringify(input)})`));
    const found = honest(t, now, report);
    // A frame with input (the pointer moving, a press) is never held.
    if (events.length) report.animating = true;
    return framed(now, report, found);
  }

  // Each decoration must line up with the real app event it shows.
  function honest(t, info, report) {
    const found = [];
    const fail = (kind, detail) => found.push(`${kind}: ${detail}`);
    if (report.caption && report.caption.text !== info.caption_text) fail("caption", `stage shows "${report.caption.text}", app "${info.caption_text}"`);
    // The spotlight is on what the app shows for its subject: the change
    // the app has selected, the zoom's subject, the highlighted line...,
    // mapped independently here.
    if (report.spot) {
      const kind = report.spot.kind;
      const tg = info.targets ?? {};
      const want = kind === "selected" ? info.selected_rect
        : kind === "focus" ? info.focus?.[0]
          : kind === "canvas" ? info.canvas
            : kind === "details" ? report.spot.app && tg[`row:${info.selected}`] && report.spot.app
              : tg[kind];
      const same = (a, b) => a && b && a.every((v, i) => Math.abs(v - b[i]) < 0.5);
      if (!same(want, report.spot.app)) fail("spotlight", `${kind}: on ${JSON.stringify(report.spot.app)}, the app has ${JSON.stringify(want)}`);
      else {
        const k = M.w / (info.screen[2] - info.screen[0]);
        // (Clipped to the app's window, as the stage clips it.)
        const [x0, y0, x1, y1] = [
          Math.max(M.x, M.x + want[0] * k), Math.max(M.y, M.y + want[1] * k),
          Math.min(M.x + M.w, M.x + want[2] * k), Math.min(M.y + M.h, M.y + want[3] * k),
        ];
        const [s0, t0, s1, t1] = report.spot.rect;
        const slack = report.spot.pad * (M.w / FRAME_APP_WIDTH[format]) * 1.2 + 2;
        const cx = (x0 + x1) / 2;
        const cy = (y0 + y1) / 2;
        const inside = cx >= s0 && cx <= s1 && cy >= t0 && cy <= t1;
        const tight = s0 >= x0 - slack && t0 >= y0 - slack && s1 <= x1 + slack && t1 <= y1 + slack;
        if (!inside || !tight) fail("spotlight", `${kind}: drawn at ${report.spot.rect.map(Math.round)}, the subject is at ${[x0, y0, x1, y1].map(Math.round)}`);
      }
    }
    for (const rt of report.ripples) {
      if (!driver.presses.some((p) => Math.abs(p.t - rt) < 1e-6)) fail("ripple", `ripple at ${rt.toFixed(2)} s with no press`);
    }
    if (report.cards) {
      if (!driver.drop || report.cards.names.join("|") !== driver.drop.names.join("|")) fail("cards", `card names ${report.cards.names} are not the dropped files`);
      if (report.cards.phase === "carried" && !driver.dragging) fail("cards", "cards carried with no drag under way");
    }
    if (report.landing && !(driver.dropped != null && t >= driver.dropped - 1e-6 && t < driver.dropped + 0.61)) fail("landing", `landing pulse at ${t.toFixed(2)} s, drop at ${driver.dropped}`);
    if (report.hover) {
      const r = info.targets[driver.hover];
      const p = info.pointer;
      if (!(r && p && p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3])) fail("hover", `hover lift off its control ${driver.hover}`);
    }
    // Every press answered by the app (the change selected, the view
    // changed) within a second; the drop by a comparison of those files.
    for (const p of driver.presses) {
      if (p.answered != null) continue;
      const ok = p.target.startsWith("row:") ? info.selected === Number(p.target.slice(4))
        : p.target === "details" ? info.view === "details"
          : p.target === "side_by_side" ? info.view === "side_by_side" : true;
      if (ok) p.answered = t;
      else if (t - p.t > 1.0) {
        p.answered = -1;
        fail("click", `${p.target} pressed at ${p.t.toFixed(2)} s: the app did not respond`);
      }
    }
    if (driver.dropped != null && driver.dropAnswered == null) {
      const names = driver.drop?.names ?? [];
      if (info.files && names.every((n) => info.files.includes(n))) driver.dropAnswered = t;
      else if (t - driver.dropped > 4) {
        driver.dropAnswered = -1;
        fail("drop", `dropped ${names} at ${driver.dropped.toFixed(2)} s: not compared`);
      }
    }
    honesty.push(...found.map((d) => ({ t, detail: d })));
    return found;
  }

  // The frame's info in the stage frame's CSS pixels, for the checks.
  function framed(info, report, found) {
    const k = M.w / (info.screen[2] - info.screen[0]);
    const P = (x, y) => [M.x + x * k, M.y + y * k];
    const R = (r) => r && [...P(r[0], r[1]), ...P(r[2], r[3])];
    return {
      ...info,
      screen: [0, 0, w, h],
      app: [M.x, M.y, M.x + M.w, M.y + M.h],
      canvas: R(info.canvas),
      focus: info.focus.map(R),
      targets: Object.fromEntries(Object.entries(info.targets).map(([n, r]) => [n, R(r)])),
      pointer: info.pointer && P(info.pointer[0], info.pointer[1]),
      pinch: info.pinch && [...P(info.pinch[0], info.pinch[1]), info.pinch[2]],
      caption: report.caption && { size: report.caption.size * h, rect: report.caption.rect, rows: report.caption.rows },
      still: info.still && !report.animating,
      stage: report,
      honesty: found,
    };
  }
  return {
    url, length, shots, gestures, requests, errors, exports, staged, honesty,
    get last() {
      return last;
    },
    // What really happened, for the sound: presses, the pickup and the drop.
    events() {
      const out = driver.presses.map((p) => ({ type: "click", t: p.t }));
      if (driver.drop && last / CLOCK >= driver.drop.pickup) out.push({ type: "pickup", t: driver.drop.pickup });
      if (driver.dropped != null) out.push({ type: "drop", t: driver.dropped });
      return out;
    },
    // Whether the next frame (`step` on) presses gesture `gi`'s control:
    // the click can come later than planned (after the list scrolled).
    clicksNext(gi, step = 1) {
      return driver.next > gi || (driver.next === gi && driver.path && (last + step) / CLOCK >= driver.path.at);
    },
    // Draws frame `f` (frames must go forward) and returns what is on
    // screen. Frames skipped on the way are drawn too where a gesture needs
    // them, so the person's actions happen exactly as when every frame is
    // drawn.
    async render(f, step = 1, cut = false) {
      for (const [a, b] of busyFrames(step)) {
        for (let k = Math.max(a, last + 1); k < Math.min(b + 1, f); k++) {
          if (k % step === 0 && k > last) await timed("gesture", () => draw(k, cut));
        }
      }
      return draw(f, cut);
    },
    async png(scale = 1) {
      const clip = scale === 1 ? undefined : { x: 0, y: 0, width: w, height: h, scale };
      return Buffer.from((await send("Page.captureScreenshot", { format: "png", clip })).data, "base64");
    },
    close() {
      shared.send("Target.closeTarget", { targetId });
    },
  };
}

const actName = (desc) => desc.match(/act: (\w+)/)?.[1] ?? "Step";
const frameOf = (t) => Math.round(t * CLOCK);

// --- Automatic checks on a still ------------------------------------------

function inside(a, b, tol = 1) {
  return a[0] >= b[0] - tol && a[1] >= b[1] - tol && a[2] <= b[2] + tol && a[3] <= b[3] + tol;
}
function overlaps(a, b) {
  return a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
}
function check(info, format, sample, target, expect = {}) {
  const [x0, y0, x1, y1] = info.screen;
  const sh = y1 - y0;
  const out = {};
  // No old and new text drawn over each other ("225").
  out.texts = { pass: info.text_overlaps === 0, detail: info.text_overlaps ? `${info.text_overlaps} changed texts drawn over each other` : "no changed texts overlap" };
  // No change headline cut off with "…".
  if (info.list) out.rows = { pass: info.cut_rows === 0, detail: info.cut_rows ? `${info.cut_rows} headline(s) cut off` : "headlines in full" };
  // Clear of a video player's controls (bottom 15 %) and buttons (top-left
  // 10 %).
  if (info.caption) {
    const ok = info.caption.rect[3] <= y1 - 0.15 * sh + 1;
    out.framing = { pass: ok, detail: ok ? "caption above the bottom 15 %" : `caption reaches ${Math.round((info.caption.rect[3] / sh) * 100)} % of the height` };
  }
  const corner = [x0, y0, x0 + 0.1 * (x1 - x0), y0 + 0.1 * sh];
  const inCorner = Object.entries(info.targets).filter(([n, r]) => !["drop", "list"].includes(n) && !n.startsWith("row:") && overlaps(r, corner)).map(([n]) => n);
  out.corner = { pass: inCorner.length === 0, detail: inCorner.length ? `in the top-left 10 %: ${inCorner.join(", ")}` : "top-left 10 % clear" };
  // The caption is about what is on screen: the view the plan expects at
  // the end of the step.
  if (sample === "settled" && expect.view) {
    out.view = { pass: info.view === expect.view, detail: `${info.view}${info.view === expect.view ? "" : `, expected ${expect.view}`}` };
  }
  // The pointer reached the control before using it.
  if (sample === "aim") {
    const p = info.pointer;
    const name = target.startsWith("drop") ? "drop" : target;
    const r = target === "pinch" ? info.pinch && [info.pinch[0] - 3, info.pinch[1] - 3, info.pinch[0] + 3, info.pinch[1] + 3] : info.targets[name];
    out.pointer = {
      pass: !!(p && r && p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]),
      detail: p && r ? `pointer at ${p.map(Math.round)} on ${target}` : `pointer ${p ? "shown" : "missing"}, ${target} ${r ? "on screen" : "not on screen"}`,
    };
  }
  // After the drop: the comparison came from the dropped example files.
  if (expect.drop && info.list && !info.phone) {
    out.dropped = { pass: info.dropped && info.example_files, detail: info.dropped ? (info.example_files ? "compared the dropped example files" : "dropped files are not the example's") : "not opened by a drop" };
  }
  if (info.caption) {
    const share = info.caption.size / sh;
    out.caption = {
      pass: share >= 0.0395 && info.caption.rows <= 2 && inside(info.caption.rect, info.screen),
      detail: `text ${(share * 100).toFixed(1)} % of the height, ${info.caption.rows} line(s)`,
    };
  } else {
    out.caption = { pass: null, detail: "no caption" };
  }
  if (sample !== "settled") {
    out.cutoff = { pass: null, detail: `${sample} (not checked)` };
  } else if (expect.spot && !["selected", "focus", "canvas"].includes(expect.spot)) {
    out.cutoff = { pass: null, detail: `the step is about the ${expect.spot}, not the drawing` };
  } else if (info.focus.length && info.canvas) {
    const cut = info.focus.filter((f) => !inside(f, info.canvas) || !inside(f, info.screen));
    const covered = info.caption ? info.focus.filter((f) => overlaps(f, info.caption.rect)) : [];
    out.cutoff = {
      pass: cut.length === 0 && covered.length === 0,
      detail: cut.length ? "the change runs off the drawing area" : covered.length ? "the caption covers the change" : "change fully visible, clear of the caption",
    };
  } else {
    out.cutoff = { pass: null, detail: info.report ? "report layout" : info.end_card ? "end card" : info.phone ? "phone" : "nothing in focus" };
  }
  if (info.list_tail != null) {
    out.list = { pass: info.list_tail, detail: info.list_tail ? "'… and N more' fully visible" : "the list is cut off before '… and N more'" };
  }
  if (format === "vertical") {
    if (info.canvas && !info.end_card && !info.phone) {
      const [c0, c1, c2, c3] = info.canvas;
      // (On the stage: of the app's window, as a full-frame recording.)
      const whole = info.app ? (info.app[2] - info.app[0]) * (info.app[3] - info.app[1]) : (x1 - x0) * sh;
      const share = ((c2 - c0) * (c3 - c1)) / whole;
      // Details open (under a second, before "Export PNG report"): its
      // extra toolbar row is the real layout; reported, not judged.
      // The large layout's timing and request lines are what its video
      // shows: in 9:16 they take the drawing's room; reported too.
      const details = !!info.targets.export_png;
      const lines = !!(info.targets.timing || info.targets.network);
      const why = details ? " (Details open, not checked)" : lines ? " (timing and request lines shown, not checked)" : "";
      out.drawing = {
        pass: details || lines ? null : share >= 0.6,
        detail: `drawing ${(share * 100).toFixed(0)} % of the frame${why}`,
      };
    } else {
      out.drawing = { pass: null, detail: info.report ? "report layout" : info.phone ? "phone" : "end card" };
    }
  }
  // Decorations that do not line up with the app's real events.
  if (info.honesty) {
    out.honesty = { pass: info.honesty.length === 0, detail: info.honesty.length ? info.honesty.join("; ") : "decorations match the app's events" };
  }
  return out;
}

// --- Stills ---------------------------------------------------------------

async function stills(format, mode) {
  const dir = join(OUT, "stills", `${mode}-${format}`);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const page = await openDemo(format, mode, QUALITY.stills.dpr);
  const samples = [];
  const stepAt = (t) => Math.max(0, page.shots.findLastIndex((s) => s.at <= t && s.view != null));
  // Per step (a shot with its view; later shots in a step only change the
  // caption): early on, and settled at its end.
  page.shots.forEach((s, i) => {
    if (s.view == null) return;
    const later = page.shots.slice(i + 1).find((n) => n.view != null);
    const next = later ? later.at : page.length;
    if (next - s.at > 2.5) samples.push({ t: s.at + 1.0, step: i, act: actName(s.desc), sample: "early" });
    // (Half a second before the next step: settled, and on the stage
    // with its spotlight still on.)
    samples.push({ t: Math.max(s.at, next - (page.staged ? 0.5 : 2 / CLOCK)), step: i, act: actName(s.desc), sample: "settled", view: s.view });
  });
  // Each gesture: the pointer on its way, and on its target just before it
  // acts (checked); a pinch half-way.
  for (const [gi, g] of page.gestures.entries()) {
    const step = stepAt(g.at);
    const act = actName(page.shots[step].desc);
    samples.push({ t: (g.from + g.at) / 2, step, act, sample: "moving" });
    samples.push({ t: g.at - 1 / CLOCK, step, act, sample: "aim", target: g.target, gi });
    if (g.until > g.at) samples.push({ t: (g.at + g.until) / 2, step, act, sample: "mid" });
  }
  // On the stage: the sting too (the storyboard's first panel).
  if (page.staged) samples.push({ t: 0.6, step: 0, act: "Sting", sample: "sting" });
  samples.sort((a, b) => a.t - b.t);
  const results = [];
  let lastFrame = -1;
  // What this video does, so only its own checks apply.
  const expect = {
    drop: page.gestures.some((g) => g.target === "drop:example" || g.target === "drop:unnamed"),
    export: page.gestures.some((g) => g.target === "export_png"),
  };
  for (const s of samples) {
    let f = Math.max(frameOf(s.t), lastFrame + 1);
    let info = await page.render(f);
    // Aim: the frame just before the press, wherever it really came.
    while (s.sample === "aim" && !page.clicksNext(s.gi) && f < frameOf(s.t) + 5 * CLOCK) info = await page.render(++f);
    lastFrame = f;
    const name = `${String(results.length).padStart(2, "0")}-${s.act}-${s.sample}-${(f / CLOCK).toFixed(1)}s.png`;
    writeFileSync(join(dir, name), await page.png());
    results.push({ file: name, time: f / CLOCK, step: s.step, act: s.act, sample: s.sample, caption: info.caption_text ?? null, checks: check(info, format, s.sample, s.target, { ...expect, view: s.view, spot: page.shots[s.step]?.spot }) });
  }
  // The report the demo exported: saved for real.
  // (Chrome finishes writing it in the background.)
  let exported = [];
  for (let i = 0; expect.export && i < 300 && exported.length === 0; i++) {
    exported = page.exports().filter((e) => e.name.endsWith(".png") && existsSync(e.file));
    if (exported.length === 0) await sleep(100);
  }
  exported = exported.map((e) => e.name);
  const last = results.at(-1);
  // Every frame drawn, not only the stills: decorations against events.
  if (page.staged) {
    const all = page.honesty;
    last.checks.decorations = { pass: all.length === 0, detail: all.length ? all.map((h) => `${h.t.toFixed(2)} s ${h.detail}`).join("; ") : `every frame: decorations match the app's events (${page.events().length} real events)` };
  }
  if (expect.export) {
    last.checks.export = { pass: exported.length === 1, detail: exported.length ? `report saved: ${exported.join(", ")}` : "no report was saved" };
  }
  page.close();
  // Contact sheet of the settled stills, and a phone-sized copy.
  const settled = results.filter((r) => r.sample === "settled" || r.sample === "aim");
  const tmp = mkdtempSync(join(tmpdir(), "sheet-"));
  settled.forEach((r, k) => copyFileSync(join(dir, r.file), join(tmp, `s${String(k).padStart(2, "0")}.png`)));
  const cols = format === "landscape" ? 3 : 6;
  const rows = Math.ceil(settled.length / cols);
  const cell = format === "landscape" ? 640 : 300;
  spawnSync("ffmpeg", ["-v", "error", "-y", "-framerate", "1", "-i", join(tmp, "s%02d.png"),
    "-vf", `scale=${cell}:-1,tile=${cols}x${rows}:padding=6:color=gray`, "-frames:v", "1", join(dir, "contact.png")]);
  spawnSync("ffmpeg", ["-v", "error", "-y", "-i", join(dir, "contact.png"), "-vf", "scale=390:-1", join(dir, "contact-phone.png")]);
  rmSync(tmp, { recursive: true, force: true });
  writeFileSync(join(dir, "checks.json"), JSON.stringify(results, null, 1));
  // The storyboard: one still per scene (the sting, then each step's
  // settled moment), labelled.
  let board = null;
  if (page.staged) {
    const scenes = [];
    const sting = results.find((r) => r.sample === "sting");
    if (sting) scenes.push({ file: sting.file, label: "Sting" });
    let n = 0;
    for (const r of results.filter((r) => r.sample === "settled")) {
      const shot = page.shots[r.step];
      if (!shot?.scene) continue;
      n++;
      // The caption as it was on screen (numbers as measured).
      const said = r.caption && shot.scene !== "end" ? `: ${r.caption}` : "";
      scenes.push({ file: r.file, label: `${n}. ${shot.scene[0].toUpperCase()}${shot.scene.slice(1)}${said}` });
    }
    board = storyboard(dir, format, scenes);
  }
  return { dir, results, requests: page.requests, errors: page.errors, url: page.url, board };
}

// A labelled contact sheet of scene stills (storyboard.png in `dir`): laid
// out as a page in the brand's type and photographed by headless Chrome
// (this ffmpeg has no text filter).
function storyboard(dir, format, scenes) {
  const cols = format === "landscape" ? 3 : 4;
  const cell = format === "landscape" ? 640 : 300;
  const font = join(ROOT, "docs", "brand", "fonts", "Lato-Regular.ttf");
  const esc = (t) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const cellH = Math.round(cell * (format === "landscape" ? 9 / 16 : 16 / 9));
  const labelH = format === "landscape" ? 52 : 64;
  const rows = Math.ceil(scenes.length / cols);
  const width = cols * cell + (cols + 1) * 10;
  const height = rows * (cellH + labelH) + (rows + 1) * 10;
  const html = `<!doctype html><meta charset="utf-8"><style>
    @font-face { font-family: Lato; src: url("file://${font}"); }
    body { margin: 0; background: #0b0f14; font: ${format === "landscape" ? 17 : 14}px/1.3 Lato, sans-serif; color: #eef2f6; }
    .g { display: grid; grid-template-columns: repeat(${cols}, ${cell}px); gap: 10px; padding: 10px; }
    .c { background: #161c24; } .c img { display: block; width: ${cell}px; height: ${cellH}px; }
    .c p { margin: 0; padding: 6px 8px; height: ${labelH - 12}px; overflow: hidden; }
    .c b { color: #f5d02a; font-weight: 400; }
  </style><div class="g">${scenes.map((sc) => {
    const [head, ...rest] = sc.label.split(": ");
    return `<div class="c"><img src="file://${join(dir, sc.file)}"><p><b>${esc(head)}</b>${rest.length ? " " + esc(rest.join(": ")) : ""}</p></div>`;
  }).join("")}</div>`;
  const page = join(dir, "storyboard.html");
  writeFileSync(page, html);
  const out = join(dir, "storyboard.png");
  spawnSync(CHROME, ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--allow-file-access-from-files",
    `--screenshot=${out}`, `--window-size=${width},${height}`, `file://${page}`], { stdio: "ignore" });
  spawnSync("ffmpeg", ["-v", "error", "-y", "-i", out, "-vf", "scale=390:-1", join(dir, "storyboard-phone.png")]);
  return { file: out, phone: join(dir, "storyboard-phone.png"), scenes };
}

// --- Videos: segments, cache, holds ---------------------------------------

// The pixels of a PNG as RGB bytes (ffmpeg decodes).
function pixels(png) {
  const r = spawnSync("ffmpeg", ["-v", "error", "-i", "-", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
    { input: png, maxBuffer: 1 << 28 });
  return r.stdout;
}

// Whether a freshly drawn frame matches a cached probe: the same picture up
// to rendering noise (no channel of any pixel more than 16 levels apart). A
// real change (a character, a line, a colour) moves pixels much further.
function alike(png, cachedFile) {
  if (!existsSync(cachedFile)) return false;
  const cached = readFileSync(cachedFile);
  if (sha(png) === sha(cached)) return true;
  const [a, b] = [pixels(png), pixels(cached)];
  if (a.length !== b.length || a.length === 0) return false;
  for (let i = 0; i < a.length; i++) if (Math.abs(a[i] - b[i]) > 16) return false;
  return true;
}

// Runs at most `n` jobs at a time.
function limiter(n) {
  let active = 0;
  const waiting = [];
  return async (job) => {
    while (active >= n) await new Promise((r) => waiting.push(r));
    active++;
    try {
      return await job();
    } finally {
      active--;
      waiting.shift()?.();
    }
  };
}

function ffmpeg(args) {
  return new Promise((resolve, reject) => {
    const p = spawn("ffmpeg", args, { stdio: ["ignore", "ignore", "pipe"] });
    let err = "";
    p.stderr.on("data", (d) => (err += d));
    p.on("close", (code) => (code === 0 ? resolve() : reject(new Error(`ffmpeg: ${err}`))));
  });
}

async function video(format, mode, kind, workers, encodeSlot) {
  const q = QUALITY[kind];
  const step = CLOCK / q.fps;
  const cacheDir = join(OUT, "cache");
  mkdirSync(cacheDir, { recursive: true });
  // The plan (steps and length) comes from the app; the first page reads it.
  const first = await timed("open", () => openDemo(format, mode, q.dpr, q.cut));
  const { shots, length } = first;
  const total = Math.ceil((length * CLOCK) / step) * step;
  const align = (f) => Math.round(f / step) * step;
  // Segments start at steps; a step shorter than 1.5 s joins the one before
  // (a segment's first frame needs the previous step's view settled).
  const starts = [0];
  for (let i = 1; i < shots.length; i++) {
    const f = align(frameOf(shots[i].at));
    if (f > starts.at(-1) && f - starts.at(-1) >= 1.5 * CLOCK && f < total) starts.push(f);
  }
  const segments = starts.map((s, k) => ({ start: s, end: k + 1 < starts.length ? starts[k + 1] : total }));
  for (const seg of segments) {
    seg.key = sha(JSON.stringify({
      v: TOOL_VERSION, format, mode, kind, q, start: seg.start, end: seg.end,
      shots: shots.filter((s) => frameOf(s.at) < seg.end),
      gestures: first.gestures.filter((g) => frameOf(g.from) < seg.end),
    }));
    seg.clip = join(cacheDir, `${seg.key}.mp4`);
    seg.meta = join(cacheDir, `${seg.key}.json`);
  }
  const counts = { drawn: 0, held: 0, probed: 0 };
  // Captions as shown (the large layout's numbers are measured as it
  // plays, so its subtitles are written from the recording).
  const shown = [];
  const encodes = [];
  const requests = [];
  const errors = [];
  const pages = [first];

  async function segmentWork(page, seg) {
    // Reuse when the current build draws the cached probe frames alike.
    if (existsSync(seg.clip) && existsSync(seg.meta)) {
      const { probes, events, honesty } = JSON.parse(readFileSync(seg.meta, "utf8"));
      seg.events = events ?? [];
      seg.honesty = honesty ?? [];
      let same = probes.every(([f]) => f > page.last);
      for (const [f, file] of probes) {
        if (!same) break;
        await timed("probe", () => page.render(f, step, q.cut));
        counts.probed++;
        same = await timed("probe", async () => alike(await page.png(q.shrink ?? 1), join(cacheDir, file)));
      }
      seg.cached = same;
      if (same) return page;
    }
    // Warm up: the end of the previous segment, so this one starts from its
    // settled view.
    if (seg.start > 0 && page.last !== seg.start - step) {
      if (page.last > seg.start - step) {
        page.close();
        page = await timed("open", () => openDemo(format, mode, q.dpr, q.cut));
      }
      for (let f = Math.max(0, seg.start - 3 * step); f < seg.start; f += step) {
        if (f > page.last) await timed("render", () => page.render(f, step, q.cut));
      }
    }
    const frames = mkdtempSync(join(tmpdir(), "seg-"));
    let n = 0;
    let prev = null;
    const pngs = new Map();
    for (let f = seg.start; f < seg.end; ) {
      const info = await timed("render", () => page.render(f, step, q.cut));
      shown.push([f, info.end_card ? END_CARD : info.caption_text]);
      const png = await timed("capture", () => page.png(q.shrink ?? 1));
      counts.drawn++;
      const hash = sha(png);
      pngs.set(f, png);
      const file = join(frames, `${String(n++).padStart(5, "0")}.png`);
      writeFileSync(file, png);
      f += step;
      // Twice the same picture with nothing moving: repeat it (hard links)
      // until the next step (or the segment's end).
      if (prev && prev.hash === hash && prev.still && info.still) {
        // Until the next step, or the next moment of a gesture (its start,
        // its click, its end): a hold must not skip a person's action.
        const moments = first.gestures.flatMap((g) => [g.from, g.at - 0.8, g.at, g.until]).map(frameOf).filter((m) => m >= f);
        const until = Math.min(seg.end, info.next == null ? seg.end : align(frameOf(info.next)),
          ...moments.map((m) => Math.floor(m / step) * step));
        for (; f < until; f += step) {
          linkSync(file, join(frames, `${String(n++).padStart(5, "0")}.png`));
          pngs.set(f, png);
          counts.held++;
        }
      }
      prev = { hash, still: info.still };
    }
    // Probe frames for next time: the first, the middle and the last.
    const all = [...pngs.keys()];
    const pick = [...new Set([all[0], all[Math.floor(all.length / 2)], all.at(-1)])];
    const probes = pick.map((f) => {
      const file = `${seg.key}-${f}.png`;
      writeFileSync(join(cacheDir, file), pngs.get(f));
      return [f, file];
    });
    // Encode in the background while rendering goes on: the numbered frames
    // exactly as before (a concat list with durations shifted frames by one).
    // What really happened in this segment (for the sound), and anything
    // dishonest found while drawing it.
    seg.events = page.staged ? page.events().filter((e) => frameOf(e.t) >= seg.start && frameOf(e.t) < seg.end) : [];
    seg.honesty = page.staged ? page.honesty.filter((h) => frameOf(h.t) >= seg.start && frameOf(h.t) < seg.end) : [];
    encodes.push(encodeSlot(() => timed("encode", async () => {
      await ffmpeg(["-v", "error", "-y", "-framerate", String(q.fps), "-i", join(frames, "%05d.png"),
        ...q.encode, "-r", String(q.fps), "-an", seg.clip]);
      writeFileSync(seg.meta, JSON.stringify({ probes, events: seg.events, honesty: seg.honesty }));
      rmSync(frames, { recursive: true, force: true });
    })));
    return page;
  }

  // Workers take the next segment in order; each keeps its own page.
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(workers, segments.length) }, async (_, w) => {
    let page = w === 0 ? first : null;
    while (next < segments.length) {
      const seg = segments[next++];
      if (!page) {
        page = await timed("open", () => openDemo(format, mode, q.dpr, q.cut));
        pages.push(page);
      }
      page = await segmentWork(page, seg);
      if (!pages.includes(page)) pages.push(page);
    }
  }));
  await Promise.all(encodes);
  for (const p of pages) {
    requests.push(...p.requests);
    errors.push(...p.errors);
    p.close();
  }
  if (mode === "big") {
    shown.sort((a, b) => a[0] - b[0]);
    const cues = [];
    for (const [f, text] of shown) {
      if (cues.length && cues.at(-1).text === text) continue;
      if (cues.length) cues.at(-1).end = f;
      cues.push({ start: f, end: total, text });
    }
    const clock = (f) => new Date((f / CLOCK) * 1000).toISOString().slice(11, 23);
    writeFileSync(join(OUT, "feature-big.vtt"), "WEBVTT\n\n" + cues.filter((c) => c.text)
      .map((c) => `${clock(c.start)} --> ${clock(c.end)}\n${c.text}\n`).join("\n"));
  }
  const prefix = kind === "final" ? "demo" : kind;
  const name = `${prefix}-${format}${suffix(mode)}.mp4`;
  const list = join(mkdtempSync(join(tmpdir(), "list-")), "list.txt");
  writeFileSync(list, segments.map((s) => `file '${s.clip}'`).join("\n"));
  const staged = first.staged;
  const silent = staged ? join(tmpdir(), `silent-${process.pid}-${name}`) : join(OUT, name);
  await ffmpeg(["-v", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-c", "copy",
    "-movflags", "+faststart", silent]);
  let sound = null;
  let honesty = [];
  if (staged) {
    // The sting, the reveal and the end card from the plan; the presses,
    // the pickup and the drop as they really happened.
    const at = (scene) => shots.find((s) => s.scene === scene)?.at;
    const events = [{ type: "sting", t: 0 }, { type: "reveal", t: at("reveal") + 0.1 }, { type: "end", t: at("end") }];
    const seen = new Set();
    for (const e of segments.flatMap((sg) => sg.events ?? [])) {
      const key = `${e.type}@${Math.round(e.t * 100)}`;
      if (!seen.has(key)) events.push(e), seen.add(key);
    }
    honesty = segments.flatMap((sg) => sg.honesty ?? []);
    sound = await mixSound(silent, join(OUT, name), total / CLOCK, events.filter((e) => e.t != null && !Number.isNaN(e.t)));
    rmSync(silent, { force: true });
  }
  return {
    name, file: join(OUT, name), segments: segments.length, reused: segments.filter((s) => s.cached).length,
    ...counts, requests, errors, sound, honesty,
  };
}

// The sound of a use-case video (tools/gen_sound.py's own music and UI
// sounds), mixed with ffmpeg: `out` with music and UI sounds, `-nomusic`
// with UI sounds only, and both stems in out/demo/audio/ for a voice-over
// recorded separately. Captions stay in the picture in every version.
async function mixSound(silent, out, length, events) {
  const dir = join(OUT, "sound");
  if (!existsSync(join(dir, "music.wav"))) {
    const r = spawnSync(join(ROOT, "tools", ".venv", "bin", "python"), [join(ROOT, "tools", "gen_sound.py"), dir]);
    if (r.status !== 0) throw new Error(`gen_sound.py: ${r.stderr}`);
  }
  const stems = join(OUT, "audio");
  mkdirSync(stems, { recursive: true });
  const base = basename(out, ".mp4");
  const sfx = join(stems, `${base}-sfx.wav`);
  const music = join(stems, `${base}-music.wav`);
  const level = { sting: 0.5, pickup: 0.4, drop: 0.7, click: 0.4, reveal: 0.4, end: 0.5 };
  const inputs = events.flatMap((e) => ["-i", join(dir, `${e.type}.wav`)]);
  const parts = events.map((e, k) => {
    const ms = Math.max(0, Math.round(e.t * 1000));
    return `[${k}:a]adelay=${ms}:all=1,volume=${level[e.type]}[e${k}]`;
  });
  await ffmpeg(["-v", "error", "-y", ...inputs, "-filter_complex",
    `${parts.join(";")};${events.map((_, k) => `[e${k}]`).join("")}amix=inputs=${events.length}:normalize=0:duration=longest,` +
    `apad=whole_dur=${length},atrim=0:${length},aformat=sample_rates=48000:channel_layouts=stereo[out]`,
    "-map", "[out]", sfx]);
  // The music low (room for a voice), in and out softly.
  await ffmpeg(["-v", "error", "-y", "-i", join(dir, "music.wav"), "-af",
    `atrim=0:${length},volume=0.2,afade=t=in:st=0:d=0.8,afade=t=out:st=${Math.max(0, length - 2)}:d=2`, music]);
  await ffmpeg(["-v", "error", "-y", "-i", silent, "-i", sfx, "-i", music, "-filter_complex",
    "[1:a][2:a]amix=inputs=2:normalize=0[a]", "-map", "0:v", "-map", "[a]", "-c:v", "copy", "-c:a", "aac", "-b:a", "160k",
    "-shortest", "-movflags", "+faststart", out]);
  const plain = out.replace(/\.mp4$/, "-nomusic.mp4");
  await ffmpeg(["-v", "error", "-y", "-i", silent, "-i", sfx, "-map", "0:v", "-map", "1:a", "-c:v", "copy", "-c:a", "aac",
    "-b:a", "160k", "-shortest", "-movflags", "+faststart", plain]);
  return { music: out, nomusic: plain, stems: [music, sfx], events };
}

// --- Review page ------------------------------------------------------------

function publish(kind, items) {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const run = join(REVIEW, "runs", `${stamp}-${kind}`);
  mkdirSync(run, { recursive: true });
  const entry = { id: `${stamp}-${kind}`, kind, time: new Date().toISOString(), note, items: [] };
  for (const item of items) {
    const copy = (src) => {
      const name = `${item.label.replace(/\W+/g, "-")}-${basename(src)}`;
      copyFileSync(src, join(run, name));
      return `runs/${entry.id}/${name}`;
    };
    const out = { label: item.label, format: item.format, mode: item.mode };
    if (item.contact) out.contact = copy(item.contact);
    if (item.phone) out.phone = copy(item.phone);
    if (item.video) out.video = copy(item.video);
    if (item.vtt) out.vtt = copy(item.vtt);
    if (item.checks) out.checks = item.checks;
    if (item.stills) out.stills = item.stills.map((s) => ({ ...s, file: copy(join(item.stillsDir, s.file)) }));
    if (item.pageDir) {
      cpSync(item.pageDir, join(run, "page"), { recursive: true });
      out.page = `runs/${entry.id}/page/index.html`;
    }
    entry.items.push(out);
  }
  const manifestFile = join(REVIEW, "manifest.json");
  const manifest = existsSync(manifestFile) ? JSON.parse(readFileSync(manifestFile, "utf8")) : [];
  manifest.unshift(entry);
  writeFileSync(manifestFile, JSON.stringify(manifest, null, 1));
  copyFileSync(join(ROOT, "tools", "review", "index.html"), join(REVIEW, "index.html"));
  return entry.id;
}

// --- Main ---------------------------------------------------------------------

// --- The demo page, previewed with drafts and checked on a phone --------

async function pagePreview(from) {
  const prefix = from === "final" ? "demo" : from;
  const dir = join(OUT, "page-preview");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  copyFileSync(join(ROOT, "web", "demo", "index.html"), join(dir, "index.html"));
  pageFiles(dir, (m, format) => join(OUT, `${prefix}-${format}-${m}.mp4`));
  const shots = join(dir, "..", "page-shots");
  rmSync(shots, { recursive: true, force: true });
  mkdirSync(shots, { recursive: true });
  const shared = await sharedBrowser();
  const results = [];
  for (const [label, width, height, dpr, mobile] of [["phone", 390, 844, 3, true], ["desktop", 1280, 900, 1, false]]) {
    const { targetId } = await shared.send("Target.createTarget", { url: "about:blank", newWindow: true });
    const ws = new WebSocket(`ws://127.0.0.1:${shared.port}/devtools/page/${targetId}`);
    await new Promise((r) => ws.addEventListener("open", r));
    let id = 0;
    const pending = new Map();
    const requests = [];
    ws.addEventListener("message", (m) => {
      const d = JSON.parse(m.data);
      if (pending.has(d.id)) {
        pending.get(d.id)(d.result ?? { error: d.error });
        pending.delete(d.id);
      }
      if (d.method === "Network.responseReceived") requests.push({ url: d.params.response.url, status: d.params.response.status });
    });
    const send = (method, params = {}) => new Promise((r) => {
      const n = ++id;
      pending.set(n, r);
      ws.send(JSON.stringify({ id: n, method, params }));
    });
    const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true })).result?.value;
    await send("Network.enable");
    await send("Page.enable");
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: dpr, mobile });
    await send("Page.navigate", { url: `${new URL(base).origin}/out/demo/page-preview/index.html` });
    await sleep(2500);
    // Every video shown at this size: how large, and whether its file,
    // poster and captions exist.
    const videos = await evaluate(`[...document.querySelectorAll("video")].filter((v) => getComputedStyle(v).display !== "none").map((v) => {
      const r = v.getBoundingClientRect();
      return { section: v.closest("section")?.id ?? "top", src: v.querySelector("source").getAttribute("src"),
               poster: v.getAttribute("poster"), track: v.querySelector("track")?.getAttribute("src") ?? null,
               x: r.left + scrollX, y: r.top + scrollY, w: r.width, h: r.height,
               muted: v.muted, inline: v.hasAttribute("playsinline"), controls: v.controls };
    })`);
    const smallest = await evaluate(`Math.min(...[...document.querySelectorAll("p, li, summary, td")].map((e) => parseFloat(getComputedStyle(e).fontSize)))`);
    const pageWidth = await evaluate("document.documentElement.scrollWidth");
    for (const v of videos) {
      const ok = async (path) => (await fetch(`${new URL(base).origin}/out/demo/page-preview/${path}`, { method: "HEAD" })).ok;
      // A feature video has no subtitle track (captions in the picture).
      const files = (await ok(v.src)) && (await ok(v.poster)) && (!v.track || (await ok(v.track)));
      const vertical = v.src.includes("vertical");
      // Large: on a phone the vertical video at the width of the screen
      // less the margins; on a wide screen the landscape video at least
      // 900 px wide.
      const large = mobile ? vertical && v.w >= width - 40 : !vertical && v.w >= 900;
      // A still at the size people see it.
      await send("Emulation.setScrollbarsHidden", { hidden: true });
      await evaluate(`scrollTo(0, ${Math.max(0, v.y - 12)})`);
      await sleep(300);
      const shot = (await send("Page.captureScreenshot", { format: "jpeg", quality: 80 })).data;
      const file = `${label}-${v.section}.jpg`;
      writeFileSync(join(shots, file), Buffer.from(shot, "base64"));
      results.push({
        file, time: 0, act: `${v.section} (${label})`, sample: `${width} px`,
        checks: {
          size: { pass: large, detail: `${vertical ? "vertical" : "landscape"} video ${Math.round(v.w)} × ${Math.round(v.h)} px on a ${width} px screen` },
          files: { pass: files, detail: files ? "video, poster and captions found" : "a video, poster or captions file is missing" },
          player: { pass: v.muted && v.inline && v.controls, detail: "muted, plays inline, with controls" },
          text: { pass: smallest >= 14, detail: `smallest text ${smallest} px` },
          width: { pass: pageWidth <= width, detail: pageWidth <= width ? "no sideways scrolling" : `page ${pageWidth} px wide` },
        },
      });
    }
    // (data: URLs are the browser's own video-control icons, not requests.)
    const outside = requests.filter((r) => !r.url.startsWith(new URL(base).origin) && !r.url.startsWith("data:"));
    results.push({ file: results.at(-1).file, time: 0, act: `requests (${label})`, sample: "network",
      checks: { network: { pass: outside.length === 0, detail: outside.length ? `requests to other sites: ${outside.map((r) => r.url).join(", ")}` : `${requests.length} requests, all to this site` } } });
    await shared.send("Target.closeTarget", { targetId });
  }
  writeFileSync(join(shots, "checks.json"), JSON.stringify(results, null, 1));
  return { dir, shots, results };
}

const started = Date.now();
// Long renders: the Mac does not go to sleep while this runs (caffeinate
// ends with this process). A closed lid still sleeps it.
spawn("caffeinate", ["-i", "-s", "-w", String(process.pid)], { stdio: "ignore", detached: true }).unref();
// At the end of every run, also a failed one: the review page's current
// address (it changes with the network), checked; started if it is down.
process.on("exit", () => {
  const server = join(ROOT, "tools", "review_server.py");
  let line = spawnSync("python3", [server, "status"], { encoding: "utf8" }).stdout?.trim() ?? "";
  if (!line.includes("- loads")) line = spawnSync("python3", [server, "start"], { encoding: "utf8" }).stdout?.trim() || line;
  console.error(line || "Review page: could not check (python3 tools/review_server.py status)");
});
if (command === "page") {
  const from = opt("from", "cut");
  const p = await pagePreview(from);
  const failed = p.results.flatMap((r) => Object.entries(r.checks).filter(([, c]) => c.pass === false).map(([k, c]) => `${r.act}: ${k} (${c.detail})`));
  const summary = { command, from, seconds: (Date.now() - started) / 1000, checked: p.results.length, failed };
  if (review) {
    summary.review = publish("page", [{
      label: `Demo page preview (${from === "final" ? "final" : `${from} draft`} videos)`, format: "page", mode: from,
      pageDir: p.dir, stills: p.results, stillsDir: p.shots,
    }]);
  }
  console.log(JSON.stringify(summary, null, 1));
  if (browser) (await browser).proc.kill();
  process.exit(0);
}
const report = [];
const items = [];
const boards = [];
const extra = [];
let network = [];
if (command !== "stills" || review) {
  // Subtitles from the same plan the app plays.
  const r = spawnSync(join(ROOT, "target", "release", "lightable"), ["demo-cues", OUT]);
  if (r.status !== 0) console.error("note: could not write subtitles (build target/release/lightable)");
}
// Variants run side by side, WORKERS browsers in all: whole variants per
// worker when there are enough of them, otherwise segments of one video
// across several.
const variants = modes.flatMap((mode) => formats.map((format) => ({ mode, format })));
const runVariant = limiter(WORKERS);
const encodeSlot = limiter(WORKERS);
const perVideo = Math.max(1, Math.floor(WORKERS / variants.length));
await Promise.all(variants.map(({ mode, format }, k) => runVariant(async () => {
  const t0 = Date.now();
  const label = `${mode} ${format}`;
  if (command === "stills") {
    const s = await stills(format, mode);
    network.push(...s.requests);
    const failed = s.results.flatMap((r) => Object.entries(r.checks).filter(([, c]) => c.pass === false).map(([key]) => `${r.file}: ${key}`));
    report[k] = { label, seconds: (Date.now() - t0) / 1000, stills: s.results.length, failed, errors: s.errors };
    items[k] = { label, format, mode, contact: join(s.dir, "contact.png"), phone: join(s.dir, "contact-phone.png"), stills: s.results, stillsDir: s.dir };
    if (s.board) {
      boards[k] = { label, format, mode, contact: s.board.file, phone: s.board.phone, stillsDir: s.dir,
        stills: s.board.scenes.map((sc) => ({ ...s.results.find((r) => r.file === sc.file), label: sc.label })) };
    }
  } else {
    const v = await video(format, mode, command, perVideo, encodeSlot);
    network.push(...v.requests);
    const vtt = join(OUT, captionsFile(mode));
    report[k] = { label, seconds: (Date.now() - t0) / 1000, video: v.name, segments: v.segments, reused: v.reused, drawn: v.drawn, held: v.held, probed: v.probed, errors: v.errors,
      ...(v.sound ? { sounds: v.sound.events.length, dishonest: v.honesty.map((h) => `${h.t.toFixed(2)} s ${h.detail}`) } : {}) };
    // A subtitle track only for the narration version (no captions in its
    // picture); the others have theirs burned in: one set ever shows.
    const track = mode === "narration" && existsSync(vtt) ? vtt : null;
    items[k] = { label: v.sound ? `${label} (music + UI sounds)` : label, format, mode, video: v.file, vtt: track };
    if (v.sound) extra.push({ label: `${label} (UI sounds, no music)`, format, mode, video: v.sound.nomusic, vtt: null });
  }
})));
const origin = new URL(base).origin;
const summary = {
  command, seconds: (Date.now() - started) / 1000, runs: report,
  network: {
    requests: network.length,
    toOtherOrigins: network.filter((r) => !r.url.startsWith(origin) && !r.url.startsWith("data:")).map((r) => r.url),
    withBody: network.filter((r) => r.method !== "GET" || r.body > 0).map((r) => `${r.method} ${r.url}`),
  },
};
if (review && boards.some(Boolean)) summary.storyboards = publish("storyboard", boards.filter(Boolean));
if (review) summary.review = publish(command, [...items, ...extra]);
if (args.includes("--profile")) summary.profile = PROFILE;
summary.workers = WORKERS;
console.log(JSON.stringify(summary, null, 1));
if (browser) (await browser).proc.kill();
process.exit(0);

// Runs the browser build in a real browser and records every network
// request the browser makes, through the browser's own debugging protocol
// (the data its developer tools show):
//
//   node tools/web_browser_test.mjs chrome|firefox URL OUTDIR [--click X,Y]... [--wait MS]
//        [--drop FILE[,FILE]] [--at MS[,MS]...] [--throttle]
//
// --throttle (Chrome only) loads the page over DevTools' "Fast 3G" network
// (562.5 ms latency, 180 kB/s down; "Slow 4G" in newer versions) and
// reports when the preview image and the app appeared (OUTDIR/timing.json).
//
// --drop hands local files to the page through the debugging connection
// (not over the network) and drops them on the app's canvas, as a user's
// drag and drop would; requests made after that are reported separately.
//
// Chrome is driven over the Chrome DevTools Protocol, Firefox over WebDriver
// BiDi; each runs in a fresh temporary profile. Clicks are real input
// events at CSS-pixel coordinates (the window is about 1400 × 1000). Writes OUTDIR/shot-N.png
// (after loading and after each click), OUTDIR/network.json (every request:
// method, URL, request body size) and OUTDIR/console.json, and lets
// downloads land in OUTDIR/downloads. Requires Node 22 (built-in WebSocket).
// Development only.

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [browser, url, outArg, ...rest] = process.argv.slice(2);
const out = resolve(outArg);
mkdirSync(join(out, "downloads"), { recursive: true });
const clicks = [];
let wait = 12000;
let drop = [];
let at = [];
let throttle = false;
for (let i = 0; i < rest.length; i++) {
  if (rest[i] === "--click") clicks.push(rest[++i].split(",").map(Number));
  if (rest[i] === "--wait") wait = Number(rest[++i]);
  if (rest[i] === "--drop") drop = rest[++i].split(",");
  if (rest[i] === "--at") at = rest[++i].split(",").map(Number);
  if (rest[i] === "--throttle") throttle = true;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const profile = mkdtempSync(join(tmpdir(), `cad-${browser}-`));
const requests = [];
const consoleLog = [];

function rpc(ws) {
  let id = 0;
  const pending = new Map();
  const listeners = [];
  ws.addEventListener("message", (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id !== undefined && pending.has(msg.id)) {
      const { ok, fail } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? fail(new Error(JSON.stringify(msg.error) + (msg.message || ""))) : ok(msg.result);
    } else {
      listeners.forEach((l) => l(msg));
    }
  });
  return {
    send: (method, params = {}, extra = {}) =>
      new Promise((ok, fail) => {
        const n = ++id;
        pending.set(n, { ok, fail });
        ws.send(JSON.stringify({ id: n, method, params, ...extra }));
      }),
    on: (l) => listeners.push(l),
  };
}

async function connect(wsUrl) {
  for (let i = 0; i < 60; i++) {
    try {
      const ws = new WebSocket(wsUrl);
      await new Promise((ok, fail) => {
        ws.addEventListener("open", ok);
        ws.addEventListener("error", fail);
      });
      return ws;
    } catch {
      await sleep(500);
    }
  }
  throw new Error("could not connect to " + wsUrl);
}

async function chrome() {
  const port = 9333;
  const proc = spawn("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", [
    `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run",
    "--no-default-browser-check", "--window-size=1400,1000", "about:blank",
  ], { stdio: "ignore" });
  let target;
  for (let i = 0; i < 60 && !target; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      target = list.find((t) => t.type === "page");
    } catch {}
    if (!target) await sleep(500);
  }
  const c = rpc(await connect(target.webSocketDebuggerUrl));
  c.on((m) => {
    if (m.method === "Network.requestWillBeSent") {
      const r = m.params.request;
      requests.push({ method: r.method, url: r.url, body: r.postData ? r.postData.length : 0 });
    }
    if (m.method === "Runtime.consoleAPICalled") consoleLog.push(m.params.args.map((a) => a.value ?? a.description).join(" "));
    if (m.method === "Runtime.exceptionThrown") consoleLog.push("EXCEPTION " + JSON.stringify(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
  });
  await c.send("Network.enable");
  await c.send("Runtime.enable");
  await c.send("Page.enable");
  await c.send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: join(out, "downloads") });
  if (throttle) {
    await c.send("Network.setCacheDisabled", { cacheDisabled: true });
    await c.send("Network.emulateNetworkConditions", {
      offline: false, latency: 562.5, downloadThroughput: 180000, uploadThroughput: 84375,
    });
    // Which WebAssembly call the page makes (streaming compile or not).
    await c.send("Page.addScriptToEvaluateOnNewDocument", { source: `
      window.__wasm = [];
      for (const name of ["instantiateStreaming", "compileStreaming", "instantiate", "compile"]) {
        const f = WebAssembly[name];
        if (f) WebAssembly[name] = function (...args) { window.__wasm.push(name); return f.apply(this, args); };
      }` });
  }
  await c.send("Page.navigate", { url });
  if (throttle) {
    // Times in ms from the start of navigation (the page's time origin).
    const timing = {};
    const evaluate = async (e) => (await c.send("Runtime.evaluate", { expression: e, returnByValue: true })).result.value;
    for (let i = 0; i < 1200 && !timing.app; i++) {
      const s = await evaluate(`[performance.now(), !!document.getElementById("preview")?.complete && document.getElementById("preview")?.naturalWidth > 0, document.readyState !== "loading" && !document.getElementById("loading"), document.getElementById("status")?.textContent, document.querySelectorAll("#changes li").length]`).catch(() => null);
      if (s) {
        if (s[1] && !timing.preview) timing.preview = Math.round(s[0]);
        if (s[4] && !timing.list) timing.list = [Math.round(s[0]), s[4]];
        if (s[2]) timing.app = Math.round(s[0]);
        if (!timing.shot && / [3-9]\d %/.test(s[3] || "")) {
          // The loading screen, a third of the way through.
          writeFileSync(join(out, "loading.png"), Buffer.from((await c.send("Page.captureScreenshot", { format: "png" })).data, "base64"));
          timing.shot = Math.round(s[0]);
        }
        const last = timing.progress?.at(-1);
        if (s[3] && s[3] !== last?.[1] && (!last || s[0] - last[0] > 1000 || !/%/.test(s[3]))) {
          (timing.progress ??= []).push([Math.round(s[0]), s[3]]);
        }
      }
      await sleep(50);
    }
    timing.wasmCalls = await evaluate("window.__wasm");
    timing.resources = await evaluate(`performance.getEntriesByType("resource").map((r) => [r.name.split("/").pop(), r.transferSize, r.decodedBodySize, Math.round(r.responseEnd)])`);
    timing.document = await evaluate(`(() => { const n = performance.getEntriesByType("navigation")[0]; return [n.transferSize, Math.round(n.responseEnd)]; })()`);
    writeFileSync(join(out, "timing.json"), JSON.stringify(timing, null, 1));
    consoleLog.push("timing " + JSON.stringify(timing));
  }
  await sleep(1000);
  const size = (await c.send("Runtime.evaluate", { expression: "[innerWidth, innerHeight, devicePixelRatio].join(' ')", returnByValue: true })).result.value;
  consoleLog.push("viewport " + size);
  return {
    eval: async (expr) => (await c.send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result.value,
    shot: async (name) => writeFileSync(join(out, name), Buffer.from((await c.send("Page.captureScreenshot", { format: "png" })).data, "base64")),
    click: async (x, y) => {
      for (const type of ["mouseMoved", "mousePressed", "mouseReleased"]) {
        await c.send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
        await sleep(60);
      }
    },
    tabs: async () => (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).filter((t) => t.type === "page").map((t) => t.url),
    close: () => proc.kill(),
  };
}

async function firefox() {
  const port = 9334;
  // Allow downloads without asking, into OUTDIR/downloads.
  writeFileSync(join(profile, "user.js"), [
    `user_pref("browser.download.dir", ${JSON.stringify(join(out, "downloads"))});`,
    `user_pref("browser.download.folderList", 2);`,
    `user_pref("browser.download.useDownloadDir", true);`,
    `user_pref("browser.download.always_ask_before_handling_new_types", false);`,
    `user_pref("datareporting.policy.dataSubmissionEnabled", false);`,
    `user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);`,
    `user_pref("browser.shell.checkDefaultBrowser", false);`,
  ].join("\n"));
  const proc = spawn("/Applications/Firefox.app/Contents/MacOS/firefox", [
    "--remote-debugging-port", String(port), "--profile", profile, "--no-remote", "--width", "1400", "--height", "1000",
  ], { stdio: "ignore" });
  await sleep(3000);
  const c = rpc(await connect(`ws://127.0.0.1:${port}/session`));
  await c.send("session.new", { capabilities: {} });
  // A normal tab (the first context in the tree can be browser UI).
  const { context } = await c.send("browsingContext.create", { type: "tab" });
  c.on((m) => {
    if (m.method === "network.beforeRequestSent") {
      const r = m.params.request;
      requests.push({ method: r.method, url: r.url, body: r.bodySize || 0 });
    }
    if (m.method === "log.entryAdded") consoleLog.push(`${m.params.level} ${m.params.text}`);
  });
  await c.send("session.subscribe", { events: ["network.beforeRequestSent", "log.entryAdded"] });
  await c.send("browsingContext.setViewport", { context, viewport: { width: 1400, height: 900 } });
  await c.send("browsingContext.navigate", { context, url, wait: "complete" });
  return {
    eval: async (expr) => (await c.send("script.evaluate", { expression: expr, target: { context }, awaitPromise: true })).result?.value,
    shot: async (name) => writeFileSync(join(out, name), Buffer.from((await c.send("browsingContext.captureScreenshot", { context })).data, "base64")),
    click: async (x, y) => {
      await c.send("input.performActions", {
        context,
        actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [
          { type: "pointerMove", x, y }, { type: "pointerDown", button: 0 }, { type: "pause", duration: 60 },
          { type: "pointerUp", button: 0 },
        ] }],
      });
    },
    close: () => proc.kill(),
  };
}

const b = browser === "firefox" ? await firefox() : await chrome();
try {
  if (at.length) {
    // Screenshots at these times (ms after loading), e.g. to follow ?demo.
    const t0 = Date.now();
    for (const [k, ms] of at.entries()) {
      await sleep(Math.max(0, ms - (Date.now() - t0)));
      await b.shot(`at-${k}-${ms}.png`);
    }
  }
  await sleep(wait);
  let afterDrop = requests.length;
  if (drop.length) {
    const files = drop.map((f) => ({ name: f.split("/").pop(), b64: readFileSync(f).toString("base64") }));
    afterDrop = requests.length;
    await b.eval(`(() => {
      const files = ${JSON.stringify(files)}.map((f) => {
        const bin = atob(f.b64);
        const bytes = new Uint8Array(bin.length);
        for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
        return new File([bytes], f.name);
      });
      const dt = new DataTransfer();
      files.forEach((f) => dt.items.add(f));
      const canvas = document.getElementById("cad_canvas");
      for (const type of ["dragenter", "dragover", "drop"]) {
        canvas.dispatchEvent(new DragEvent(type, { dataTransfer: dt, bubbles: true, cancelable: true }));
      }
      return files.length;
    })()`);
    await sleep(8000);
  }
  await b.shot("shot-0.png");
  let n = 0;
  for (const [x, y] of clicks) {
    await b.click(x, y);
    await sleep(3000);
    await b.shot(`shot-${++n}.png`);
  }
  const title = await b.eval("document.title");
  // Tabs open at the end (e.g. one opened by the Feedback button).
  const tabs = b.tabs ? await b.tabs() : [];
  const report = await b.eval("document.getElementById('report')?.textContent || ''");
  writeFileSync(join(out, "network.json"), JSON.stringify(requests, null, 1));
  writeFileSync(join(out, "console.json"), JSON.stringify(consoleLog, null, 1));
  const origin = new URL(url).origin;
  const foreign = requests.filter((r) => !r.url.startsWith(origin) && !r.url.startsWith("blob:") && !r.url.startsWith("data:"));
  const sending = requests.filter((r) => r.method !== "GET" || r.body > 0);
  console.log(JSON.stringify({
    browser, title, tabs, requests: requests.length,
    requestsAfterDrop: drop.length ? requests.slice(afterDrop).map((r) => `${r.method} ${r.url}`) : "no drop", toOtherOrigins: foreign.map((r) => r.url),
    nonGetOrWithBody: sending.map((r) => `${r.method} ${r.url}`), report, console: consoleLog.slice(0, 20),
  }, null, 1));
} finally {
  b.close();
}

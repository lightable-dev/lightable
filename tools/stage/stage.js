// The brand stage around the real app (see index.html). Everything drawn
// here is a pure function of the frame tools/demo.mjs sets, of what the app
// reports about itself (__lightableFrameInfo, __lightableDemoShots) and of
// the person's real input the recorder performed (pointer, presses, the
// drop): decorations never show an action that did not happen.
"use strict";
(() => {
  const CLOCK = 30; // frames a second, as the app's demo clock
  const STING = 1.2; // seconds; the app's first step holds this much longer
  const params = new URLSearchParams(location.search);
  const format = params.get("format") === "vertical" ? "vertical" : "landscape";
  // Geometry in CSS pixels of the frame (960 x 540 or 540 x 960, recorded
  // at 2x). The window fills most of the frame; its app keeps the frame's
  // top-left 10 % free of controls (the app is told how far in to start),
  // and the app keeps its captions above the frame's bottom 15 %, where a
  // player puts its controls.
  const G = format === "landscape"
    ? { w: 960, h: 540, wx: 24, wy: 10, ww: 912, tb: 22, ih: 484, fy: 516, lockup: 14, grid: 24,
        card: [176, 40, 13], mark: 118, word: 44, gap: 18, end: [58, 30, 19, 24], endGap: 24,
        tray: [0.74, 0.62], from: [0.36, 0.3], finger: 0, chip: 10, addr: 12 }
    : { w: 540, h: 960, wx: 8, wy: 8, ww: 524, tb: 22, ih: 902, fy: 932, lockup: 16, grid: 24,
        card: [196, 46, 15], mark: 128, word: 48, gap: 22, end: [66, 32, 20, 26], endGap: 28,
        tray: [0.58, 0.68], from: [0.5, 0.25], finger: 26, chip: 10, addr: 8 };
  const root = document.documentElement.style;
  const px = (v) => `${v}px`;
  Object.entries({
    w: G.w, h: G.h, wx: G.wx, wy: G.wy, ww: G.ww, tb: G.tb, ih: G.ih, fy: G.fy, lockup: G.lockup,
    grid: G.grid, "card-w": G.card[0], "card-h": G.card[1], "card-font": G.card[2], mark: G.mark,
    word: G.word, "sting-gap": G.gap, "end-lockup": G.end[0], "end-1": G.end[1], "end-2": G.end[2],
    "end-3": G.end[3], "end-gap": G.endGap, chip: G.chip, "addr-gap": G.addr,
    cx: G.wx + G.ww / 2, cy: G.wy + (G.tb + G.ih) / 2,
  }).forEach(([k, v]) => root.setProperty(`--${k}`, px(v)));

  // The app, sized for its window: the same layout as a full-frame
  // recording at 1.75 CSS pixels a point, scaled to the window's width.
  const zoom = (1.75 * G.ww) / G.w;
  // ...also at the camera's closest (2 % pushed in, around the window's
  // centre).
  const PUSH = 0.02;
  const cx = G.wx + G.ww / 2;
  const inset = Math.max(0, Math.ceil(cx - (cx - 0.1 * G.w) / (1 + PUSH) - G.wx) + 4);
  const iframe = document.getElementById("app");
  iframe.src = `../../target/web/index.html?${params.get("app") ?? "demo&record"}&stage&zoom=${zoom}&frameh=${G.h}&inset=${inset}`;
  window.__app = () => iframe.contentWindow;

  const $ = (id) => document.getElementById(id);
  const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
  const easeOut = (k) => 1 - (1 - k) ** 3;
  const easeInOut = (k) => (k < 0.5 ? 4 * k * k * k : 1 - (-2 * k + 2) ** 3 / 2);

  // Animations (Web Animations API), paused: each frame sets their time.
  const anim = (el, frames, duration, delay = 0, easing = "cubic-bezier(.2,.8,.2,1)") => {
    const a = el.animate(frames, { duration, delay, easing, fill: "both" });
    a.pause();
    return a;
  };
  const sheets = document.querySelectorAll(".sheet");
  const sting = [
    anim(sheets[0], [{ transform: "translateX(-55%)", opacity: 0 }, { transform: "none", opacity: 1 }], 420),
    anim(sheets[1], [{ transform: "translateX(55%)", opacity: 0 }, { transform: "none", opacity: 1 }], 420),
    anim(sheets[2], [{ opacity: 0 }, { opacity: 1 }], 240, 330, "ease-out"),
    anim($("sting-word"), [{ opacity: 0, transform: "translateY(12px)" }, { opacity: 1, transform: "none" }], 360, 420),
    anim($("sting"), [{ opacity: 1, transform: "scale(1)" }, { opacity: 0, transform: "scale(1.05)" }], 330, (STING - 0.33) * 1000, "ease-in"),
  ];
  const end = [
    anim($("endcard"), [{ opacity: 0 }, { opacity: 1 }], 420, 0, "ease-out"),
    anim($("end-lockup"), [{ opacity: 0, transform: "scale(0.9)" }, { opacity: 1, transform: "none" }], 500, 120),
    ...["end-1", "end-2", "end-3"].map((id, k) =>
      anim($(id), [{ opacity: 0, transform: "translateY(14px)" }, { opacity: 1, transform: "none" }], 420, 260 + 110 * k)),
  ];
  // Captions: a plain fade.
  const captionIn = anim($("caption"), [{ opacity: 0 }, { opacity: 1 }], 300, 0, "ease-out");
  const setTime = (list, seconds) => list.forEach((a) => (a.currentTime = Math.max(0, seconds * 1000)));

  // The plan's steps (the first shot of each) with their scenes.
  let steps = null;
  const plan = () => {
    if (!steps) {
      const shots = JSON.parse(__app().__lightableDemoShots || "[]");
      steps = { shots, steps: shots.filter((s) => s.view !== null) };
    }
    return steps;
  };
  const sceneAt = (name) => plan().steps.find((s) => s.scene === name)?.at;

  // The camera: the window comes in after the sting, pushes in slightly as
  // the result comes (0.8 s, then still) and eases back as the payoff
  // starts: it never moves while a scene's point is shown.
  function scale(t) {
    const intro = 0.96 + 0.04 * easeOut(clamp((t - (STING - 0.3)) / 0.45));
    const r0 = sceneAt("reveal");
    const p0 = sceneAt("payoff");
    let push = 0;
    if (r0 != null && t >= r0) push = PUSH * easeInOut(clamp((t - r0) / 0.8));
    if (p0 != null && t >= p0) push = PUSH * (1 - easeInOut(clamp((t - p0) / 0.8)));
    return intro * (1 + push);
  }
  function camera(f) {
    const t = f / CLOCK;
    const cam = $("camera");
    cam.style.transform = `scale(${scale(t)})`;
    cam.style.opacity = String(clamp((t - (STING - 0.3)) / 0.45));
    const r = iframe.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  }

  const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const ARROW = '<svg class="arrow" viewBox="0 0 20 12" aria-hidden="true"><path d="M1 6h15M11 1.5l5.5 4.5-5.5 4.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>';
  // Natural line breaks: a number stays with its unit or noun ("13.5 mm",
  // "14 changes"), "Rev B" and "Ø8 hole" stay together, and so do the
  // short words of a phrase ("from 11 mm", "to 13.5 mm"); CSS balances the
  // two lines. Only the scene's key number in the highlight colour.
  const NB = "\u00a0";
  const keepTogether = (text) => text
    .replace(/(\b(?:Rev|Ra|layer)) (\S)/g, `$1${NB}$2`)
    .replace(/([±Ø]?\d[\d,.]*) (?=\S)/g, `$1${NB}`)
    .replace(/\b(a|an|the|from|to|of|in|on|by|and|just) (?=\S)/gi, `$1${NB}`);
  const markup = (text, key) => {
    const together = keepTogether(text);
    const k = key ? keepTogether(key) : null;
    const at = k ? together.indexOf(k) : -1;
    const html = at < 0 ? escape(together)
      : escape(together.slice(0, at)) + `<span class="n">${escape(k)}</span>` + escape(together.slice(at + k.length));
    return html.replace(/→/g, ARROW);
  };

  const POINTER = '<svg width="14" height="21" viewBox="0 0 14 21"><path d="M0 0L0 17L4.2 13.2L7 19.6L9.6 18.5L6.9 12.2L12.4 12.2Z" fill="#fff" stroke="#0b0f14" stroke-width="1.3" stroke-linejoin="round"/></svg>';
  const FINGER = `<svg width="${2 * G.finger}" height="${2 * G.finger}" viewBox="0 0 2 2"><circle cx="1" cy="1" r="0.94" fill="rgba(255,255,255,0.35)" stroke="#fff" stroke-width="0.09"/></svg>`;
  const pointer = $("pointer");
  pointer.innerHTML = G.finger ? FINGER : POINTER;
  if (G.finger) pointer.classList.add("finger");

  const show = (el, on) => (el.style.display = on ? "" : "none");
  let lastCaption = null;

  // Draws the decorations of frame f. `input` is what the recorder really
  // did: pointer (CSS px of the frame), down, hover (on the control it is
  // about to click), presses [{t, x, y, target}] and the drop {from,
  // pickup, at, point, names}. Returns what was drawn, for the checks.
  function paint(f, input) {
    const t = f / CLOCK;
    const info = JSON.parse(__app().__lightableFrameInfo);
    const r = iframe.getBoundingClientRect();
    const k = r.width / (info.screen[2] - info.screen[0]);
    const map = (x, y) => [r.left + x * k, r.top + y * k];
    const report = { t, animating: false, sting: t < STING, end: false, caption: null, pulse: null, ripples: [], cards: null, landing: false, hover: !!input.hover };

    // Intro sting.
    show($("sting"), t < STING);
    setTime(sting, t);
    if (t < STING + 0.5) report.animating = true;

    // End card: the app's own end-card words, laid out in three lines.
    const endAt = sceneAt("end");
    const ending = info.end_card || (endAt != null && t >= endAt);
    show($("endcard"), ending);
    if (ending) {
      $("endcard").style.display = "flex";
      const words = plan().steps.find((s) => s.scene === "end")?.caption ?? "";
      const parts = words.split(/(?<=\.)\s+/);
      $("end-1").textContent = parts.slice(0, 2).join(" ");
      $("end-2").textContent = parts.slice(2, -1).join(" ");
      $("end-3").textContent = parts.at(-1) ?? "";
      setTime(end, t - (endAt ?? t));
      report.end = true;
      if (t - (endAt ?? t) < 0.9) report.animating = true;
    }

    // The caption: the app's words (numbers as measured), where the app
    // keeps the drawing clear, coming in when its step starts.
    const text = info.caption_text;
    const capEl = $("caption");
    if (text && !ending && t >= STING + 0.1) {
      const shot = plan().shots.filter((s) => s.caption && s.at <= t + 1e-6).at(-1);
      const start = Math.max(shot ? shot.at : 0, STING + 0.1);
      const span = $("caption-text");
      if (lastCaption !== `${text}|${info.caption_key}`) {
        span.innerHTML = markup(text, info.caption_key);
        lastCaption = `${text}|${info.caption_key}`;
      }
      const box = info.caption;
      const canvas = info.canvas ?? info.screen;
      const [cl] = map(canvas[0], 0);
      const [cr] = map(canvas[2], 0);
      const area = box ? { cx: map((box.rect[0] + box.rect[2]) / 2, 0)[0], bottom: map(0, box.rect[3])[1] }
        : { cx: (cl + cr) / 2, bottom: r.bottom - 0.12 * r.height };
      let size = (box ? box.size : 0.045 * (G.h / zoom)) * k;
      const smallest = 0.04 * G.h;
      capEl.style.display = "block";
      capEl.style.width = px((cr - cl) * 0.98);
      span.style.maxWidth = px((cr - cl) * 0.98);
      const rows = () => Math.round((span.getBoundingClientRect().height - 0.5 * size - 2) / (1.22 * size));
      for (;;) {
        span.style.fontSize = px(size);
        if (rows() <= 2 || size <= smallest) break;
        size = Math.max(smallest, size * 0.95);
      }
      capEl.style.left = px(area.cx - ((cr - cl) * 0.98) / 2);
      capEl.style.top = px(area.bottom - span.getBoundingClientRect().height);
      captionIn.currentTime = Math.max(0, (t - start) * 1000);
      const b = span.getBoundingClientRect();
      report.caption = { text, rows: rows(), size: size / G.h, rect: [b.left, b.top, b.right, b.bottom] };
      if (t - start < 0.4) report.animating = true;
    } else {
      capEl.style.display = "none";
    }

    // The spotlight: once the step's action is done (its click answered,
    // its zoom finished), the app around what the step is about is softly
    // dimmed; it lifts just before the next step.
    report.spot = spotlight(t, info, input, ending);
    if (report.spot?.fading) report.animating = true;

    // Where the viewer is: 1 Drop, 2 Result, 3 Proof.
    const scene = plan().steps.filter((s) => s.at <= t + 1e-6).at(-1)?.scene;
    const order = { action: 0, reveal: 1, proof: 2, payoff: 3 };
    const now = order[scene] ?? -1;
    $("chips").style.visibility = now >= 0 && !ending ? "visible" : "hidden";
    [...$("chips").children].forEach((chip, k) => {
      chip.className = k === now ? "on" : k < now ? "done" : "";
    });
    report.chip = now >= 0 && now < 3 ? now + 1 : null;

    // Click ripples where the recorder really pressed.
    const ripples = $("ripples");
    ripples.textContent = "";
    for (const p of input.presses ?? []) {
      const age = t - p.t;
      if (age < 0 || age >= 0.45) continue;
      const k2 = easeOut(age / 0.45);
      const rad = 6 + 24 * k2;
      const el = document.createElement("div");
      el.className = "ripple";
      Object.assign(el.style, { left: px(p.x - rad), top: px(p.y - rad), width: px(2 * rad), height: px(2 * rad), opacity: String(1 - k2) });
      ripples.appendChild(el);
      report.ripples.push(p.t);
      report.animating = true;
    }

    // The two files: cards slide in, the pointer picks them up (the real
    // drag starts there), carries them to the window and drops them (the
    // real drop), with a soft landing pulse.
    const cards = $("cards");
    cards.textContent = "";
    const landing = $("landing");
    landing.style.display = "none";
    const d = input.drop;
    if (d && t >= d.from && t < d.at + 0.6) {
      report.animating = true;
      let phase;
      let pos;
      let sc = 1;
      let op = 1;
      const tray = [G.tray[0] * G.w, G.tray[1] * G.h];
      if (t < d.pickup) {
        phase = "arriving";
        const k3 = easeOut(clamp((t - d.from) / 0.45));
        pos = [tray[0] + (1 - k3) * G.from[0] * G.w, tray[1] + (1 - k3) * G.from[1] * G.h];
        op = k3;
      } else if (t < d.at) {
        phase = "carried";
        pos = [input.pointer[0] + 12, input.pointer[1] + 14];
        sc = 0.92;
      } else {
        phase = "dropped";
        const k4 = clamp((t - d.at) / 0.3);
        pos = [d.point[0] + 12 * (1 - k4), d.point[1] + 14 * (1 - k4)];
        sc = 0.92 * (1 - 0.5 * k4);
        op = 1 - k4;
        const lk = easeOut(clamp((t - d.at) / 0.6));
        const rad = 10 + 70 * lk;
        Object.assign(landing.style, { display: "block", left: px(d.point[0] - rad), top: px(d.point[1] - rad), width: px(2 * rad), height: px(2 * rad), opacity: String(1 - lk) });
        report.landing = true;
      }
      if (op > 0.01) {
        d.names.forEach((name, n) => {
          const el = document.createElement("div");
          el.className = "card";
          el.innerHTML = `<span class="doc">DXF</span><span>${escape(name)}</span>`;
          // Stacked so both names read.
          const off = n === 0 ? [0, 0] : [14, G.card[1] + 6];
          const tilt = phase === "carried" ? (n === 0 ? -4 : 3) : (n === 0 ? -2 : 2);
          Object.assign(el.style, {
            left: px(pos[0] + off[0] * sc), top: px(pos[1] + off[1] * sc), opacity: String(op),
            transform: `scale(${sc}) rotate(${tilt}deg)`, transformOrigin: "0 0",
            boxShadow: phase === "carried" ? "0 14px 30px rgba(0,0,0,0.55)" : "",
          });
          cards.appendChild(el);
        });
      }
      report.cards = { phase, names: d.names.slice() };
    }

    // The pointer: eased by the recorder; lifts over a control it is about
    // to press, dips while pressed.
    if (input.pointer && !ending && t >= STING) {
      pointer.style.display = "block";
      const s = input.down ? 0.88 : input.hover ? 1.12 : 1;
      const [x, y] = input.pointer;
      pointer.style.transform = G.finger
        ? `translate(${x - G.finger}px, ${y - G.finger}px) scale(${s})`
        : `translate(${x}px, ${y}px) scale(${s * 1.15})`;
      pointer.style.filter = input.hover ? "drop-shadow(0 4px 6px rgba(0,0,0,0.5))" : "drop-shadow(0 2px 3px rgba(0,0,0,0.4))";
    } else {
      pointer.style.display = "none";
    }

    // Camera moving.
    const r0 = sceneAt("reveal");
    const p0 = sceneAt("payoff");
    if ((r0 != null && t >= r0 && t < r0 + 1.7) || (p0 != null && t >= p0 && t < p0 + 1.1)) report.animating = true;
    return JSON.stringify(report);
  }

  // The spotlight's subject in the app's points, by kind (the step's
  // "spot"), from what the app reports this frame.
  function subject(kind, info, step) {
    const t = info.targets ?? {};
    switch (kind) {
      case "selected": {
        const row = Number(/Tap\((\d+)\)/.exec(step.desc)?.[1]);
        return info.selected === row ? info.selected_rect : null;
      }
      case "focus":
        return info.focus?.[0] ?? null;
      case "canvas":
        return info.canvas;
      case "details": {
        const row = t[`row:${info.selected}`];
        const next = t[`row:${info.selected + 1}`];
        const list = t.list;
        if (!row || !list) return null;
        return [list[0], row[1], list[2], Math.min(list[3], next ? next[1] - 2 : list[3])];
      }
      default:
        return t[kind] ?? null;
    }
  }
  const spotEl = $("spot");
  function spotlight(t, info, input, ending) {
    spotEl.style.display = "none";
    const { steps: all, shots } = plan();
    const k = all.findLastIndex((s) => s.at <= t + 1e-6);
    const step = all[k];
    if (!step?.spot || ending) return null;
    const next = all[k + 1]?.at ?? Infinity;
    // Done: its last real press answered (0.25 s on), its pinch finished,
    // or 0.3 s in for a step with no gesture.
    const gestures = JSON.parse(__app().__lightableDemoGestures || "[]").filter((g) => g.from >= step.at - 1e-6 && g.from < next);
    const presses = (input.presses ?? []).filter((p) => p.t >= step.at - 1e-6 && p.t < next);
    let done = step.at + 0.3;
    for (const g of gestures) {
      if (g.target === "pinch") done = Math.max(done, g.until + 0.2);
      else if (!g.target.startsWith("drop")) {
        const p = presses.at(-1);
        if (!p) return null;
        done = Math.max(done, p.t + 0.25);
      }
    }
    // ...and only once the app's view is still (no zoom under way).
    if (t < done || !info.still) return null;
    const rect = subject(step.spot, info, step);
    if (!rect) return null;
    const kk = G.ww / (info.screen[2] - info.screen[0]);
    const pad = 8;
    const x0 = Math.max(0, rect[0] * kk - pad);
    const y0 = Math.max(0, rect[1] * kk - pad);
    const x1 = Math.min(G.ww, rect[2] * kk + pad);
    const y1 = Math.min(G.ih, rect[3] * kk + pad);
    if (x1 - x0 < 4 || y1 - y0 < 4) return null;
    const fadeIn = clamp((t - done) / 0.35);
    const fadeOut = clamp((next - 0.05 - t) / 0.25);
    Object.assign(spotEl.style, {
      display: "block", left: px(x0), top: px(y0), width: px(x1 - x0), height: px(y1 - y0),
      opacity: String(Math.min(fadeIn, fadeOut)),
    });
    const b = spotEl.getBoundingClientRect();
    return { kind: step.spot, rect: [b.left, b.top, b.right, b.bottom], app: rect, pad, fading: fadeIn < 1 || fadeOut < 1 };
  }

  window.__stage = {
    format,
    geometry: G,
    camera,
    paint,
    // The tray where the file cards wait (CSS px of the frame).
    tray: () => [G.tray[0] * G.w, G.tray[1] * G.h],
    ready: () => document.fonts.status === "loaded" && [...document.images].every((i) => i.complete),
  };
})();

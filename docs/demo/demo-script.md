# The 40-second demo (`?demo`, `--demo`)

Situation: a manufacturing engineer gets Rev C of a steel bracket drawing
and wants to know what changed since Rev B. Viewers watch a person use
Lightable: a pointer (a fingertip circle in 9:16) moves to each control
before it is used, and every action goes through the app's normal input.
`tools/demo.mjs` performs them as browser input events (a file drag and
drop, mouse moves and clicks, a ctrl + wheel pinch, small wheel steps to
scroll the list); playing live (`?demo`, `--demo`), the app feeds the same
events to its own input. The plan (`crates/lightable/src/demo.rs`) says
what happens when; nothing is placed on screen by the demo that a person
did not do, apart from the captions, the pointer itself, a highlight
around the colour legend, and the phone outline.

Every number and headline comes from comparing the example's two DXF files,
the files that are then dropped in (a check confirms the dropped files are
the example's).

| Start | On screen (gesture) | Caption |
|---|---|---|
| 0 s | Rev C open on its own, as it came in | "Rev C just came in. What changed?" |
| 3 s | The pointer drags the two example files in from the edge; the page shows where to drop them; dropped at 4.8 s, read in the page, compared; the file names appear in the summary | "Drop in two revisions. No install, no upload." |
| 7 s | The overlay and the change list; the colour legend highlighted for 2.7 s | "10 changes. Blue added, red deleted, yellow changed." |
| 11 s | The pointer goes to the cloud and pinches in (ctrl + wheel, 11.8–13 s) until the cloud and bolt note fill the view above the caption | "The cloud marks 1 change. The other 9 weren't clouded." |
| 16 s | Taps the first changed hole in the list (16.8 s); it zooms there and the row shows its full headline. Then, scrolling the list first where the row is out of view (9:16), taps the 20 → 25 mm dimension (20.8 s) | "Every change in plain words. Tap to zoom." |
| 24 s | Clicks "Side by side" (24.7 s) | "Before and after, side by side." |
| 28 s | Clicks "Details" (28.6 s), then "Export PNG report" (29.5 s): the report is made and really saved (recording: into the recorder's folder; live: made but not downloaded), and stays on screen | "Share a review report." |
| 32 s | The same comparison in a phone outline, laid out as on a phone (320 points wide) | "Works on your phone too." |
| 36–40 s | End card | "Free. Open source. Your files are read in your browser, never uploaded. lightable.dev" |

"10 changes" is `Diff::design_changes().count()`; the summary line above
the caption says "10 changes found between bracket-rev-b.dxf and
bracket-rev-c.dxf". The legend's words are "added", "deleted", "changed".
The taps are chosen by `Compare::demo_picks`: changes the cloud does not
surround, a circle and a dimension first.

**The cloud caption.** "The other 9 weren't clouded" is said only because
the bracket example was drawn that way (`Example::cloud_by_design`; the
example's note on screen: "Example: the drafter clouded only the bolt
note."). The count inside the cloud is computed for the demo only. For any
other comparison the step says "The revision cloud, up close." (or that
there is no cloud) and nothing about what is or isn't clouded; the test
`only_the_example_built_that_way_says_the_others_were_not_clouded` checks
it. Lightable does not check clouds.

Captions are 4.5 % of the screen height (smaller only to fit two lines),
at the bottom of the drawing; the app's fits and zooms keep that band clear
during the demo.

## Use-case videos (the demo page)

`?demo&feature=unclouded|tiny|resave|blocks|big` (`--demo --feature
ID`): 20–30 s each, the same recorder, pointer and rules; each proves one
capability on its own drawing (made by `tools/gen_usecases.py`; the large
layout by `crates/lightable/src/depot.rs`), with an expected-change
inventory next to it in `crates/lightable/assets`. The plans are in
`crates/lightable/src/demo.rs` (`feature_plan`); captions in
`out/demo/feature-ID.vtt` (the big one's from the recording, since its
numbers are measured as it plays).

| Video | Drawing | Steps |
|---|---|---|
| unclouded | gearbox cover, Rev B → Rev C | 14 changes, 1 clouded; the holes, the callout and the section edges were not; tap them; Details |
| tiny | locating plate, Rev B → Rev C | pinch onto the dowel hole; "moved 0.5 mm right"; Details with the exact values; the tolerance note ±0.1 → ±0.05 |
| resave | drive shaft, Rev D → Rev E (a simulated re-export: new handles, shuffled order, older DXF version) | drop both (the names give the order); 1 change, Ra 1.6 → Ra 0.8; Details: "17 objects redrawn the same: not changes" |
| blocks | flat, Rev A → Rev B | the door block edit at all 7 doors; the wall moved 300 mm; Side by side |
| big | warehouse layout, 23,181 / 23,182 objects | drop both; the timing line (with the machine) and the request counter (Resource Timing); a change |

```bash
node tools/demo.mjs stills --modes features
node tools/demo.mjs cut --modes features
node tools/demo.mjs page            # the page with the cut drafts, checked at 390 px and 1280 px
node tools/demo.mjs approve --mode features   # finals only, after approval
```

## What the demo does not claim

- The big video's counter counts the browser's Resource Timing entries
  after the files were opened, nothing more; the page explains how to
  check every request in the Network tab. Times are measured on the
  recording machine while recording, and say so.

- Lightable does not check whether a revision cloud covers every change.
  The cloud sentence is narration about this example, computed for the
  demo only.
- It does not say whether a drawing is ready for manufacturing, whether a
  design is safe, or whether a change is correct.
- It compares drawing data (entities, their values, layers, blocks, units).
  It does not understand the part. "Not compared yet" lists what it skips.

## Other examples

`?demo&example=plan` plays the same script on the floor plan: there is no
revision cloud, so that step says so (no pinch), and the taps are two of
the plan's changes.

## Voice-over mode and video files

`?demo&narration` plays the voice-over version for the bracket example: the
nine steps of `docs/demo/voiceover.md`, each long enough to speak its line
at about 2.5 words a second plus settling time (and never shorter than the
normal step), the same gestures, no captions in the picture (about 56 s).
Other drawings fall back to the normal demo.

Subtitles and the cue sheet come from the same plan the app plays
(`crates/lightable/src/demo.rs`):

```bash
cargo run --release -p lightable -- demo-cues out/demo
```

writes `demo.srt`/`.vtt` (normal demo: the on-screen captions and the end
card), `demo-narration.srt`/`.vtt` (the voice-over lines) and
`cue-sheet.md`. Times count from the first frame of the matching video.

## Video pipeline: stills, drafts, finals

Everything is made from the web build (`tools/build_web.sh`, served by
`python3 tools/web_test_server.py`) by `tools/demo.mjs`, and published to
the private review page (below). The workflow: stills and checks first,
then draft videos, then final videos, each reviewed on a phone; only an
approved final goes to lightable.dev/demo.

```bash
node tools/demo.mjs stills --note "what changed"
```

For each step a frame a second in and one just before the next step; for
each gesture the pointer on its way and on its target just before it acts
(and a pinch half-way); for both formats and both timings, with a contact
sheet per set and a 390 px wide copy for phones; about 2.5 minutes for all
four sets. Each still is checked automatically, pass or fail:

- caption text at least 4 % of the frame height, at most two lines, inside
  the frame;
- the change (or cloud, or sheet) the step is about fully inside the
  drawing area and not covered by the caption;
- the pointer on the control (or the pinch point) just before it acts;
- after the drop, the comparison came from the dropped files, and they are
  the example's;
- the exported report was saved (the browser reports the download);
- in 9:16, the drawing area at least 60 % of the frame (reported, not
  judged, during the 0.9 s Details is open before the export: its extra
  toolbar row is the real layout).

```bash
node tools/demo.mjs draft --note "what changed"   # 960 × 540 / 540 × 960, 15 fps, VideoToolbox
node tools/demo.mjs final --note "what changed"   # 1920 × 1080 / 1080 × 1920, 30 fps, x264
```

`--modes normal` or `--formats vertical` limit a run. How it is fast:

- `?demo&record` puts the demo on a frame clock the tool sets (demo time =
  frame ÷ 30), every zoom starts at its scheduled time, and egui's own
  animations and tooltips are off. The tool sends each input event and
  waits for the draw it causes (`__lightableDraws`); while the app is busy
  outside the clock (the browser reading dropped files, the report being
  captured) it draws the same frame again. Frames a gesture needs are
  always drawn in order; other frames are skipped where the picture holds,
  so stills jump to the moments that matter. Pinch steps are ctrl + wheel
  (applied at once by egui) and list scrolling uses steps under 8 points
  (also applied at once): both independent of the wall clock.
- Drafts render at 2× like finals and are captured at half size: on a
  high-density screen Chrome reports 2× device pixels to the page even
  when 1× is emulated, which laid the old 1× drafts out at half scale (and
  would have put the pointer beside its target). The tool also checks, per
  page, that the app's width times its zoom is the page's width.
- When two frames in a row are identical and nothing is moving, the frame
  is drawn once and repeated until the next step.
- Videos are cut into segments at the steps. A segment is cached under its
  timeline and settings; before reuse the current build redraws three
  probe frames (start, middle, end), which must match the cached ones
  exactly. Only steps that look different are recorded again; ffmpeg joins
  the segments.
- One Chrome per run; each video gets its own window. Segments of a video,
  and videos, are spread over workers (`--workers N`; by default 2 for
  stills and cut drafts, otherwise half the physical cores, at most 4).
  On a 2-core Mac, finals and drafts run best with one worker: x264 and
  Chrome already fill both cores.
- Each frame is asked for with a single redraw nudge (repeated nudges drew
  twice and made a few frames differ between runs).
- The test server lets the browser cache the WebAssembly for 60 s, so it is
  compiled once per run.

```bash
node tools/demo.mjs cut     # steps and gestures without animation, 15 fps, ultrafast: content review
```

`--profile` prints where the time went (launch, load, capture, encode).

Tried and dropped (measured on the 2-core Intel Mac):

- `HeadlessExperimental.beginFrame`: not supported on macOS.
- Reading the canvas pixels in the page: slower (≈ 550 ms a frame against
  ≈ 150 ms for a screenshot) and not identical (PSNR 49.6 dB).
- Headless Chrome: slower, and draws with a different GL path (PSNR 14.7 dB
  against the windowed frames).
- Reusing normal-timing frames in the narration videos: the frames match
  within tolerance, but x264 amplifies the tiny differences up to 38 levels
  in the encoded video, so each timing is drawn on its own.

`.github/workflows/demo-videos.yml` renders the four finals in parallel on
demand (manual trigger only) and uploads them as workflow artifacts; it
never deploys. Its frames come from a software renderer and are not
identical to the Mac's; use them for review only.

## Private review page

```bash
python3 tools/review_server.py install    # keep it running; prints http://<this Mac>:8766/
python3 tools/review_server.py status     # the current URL, checked
python3 tools/review_server.py uninstall
```

Serves `out/review/` (git-ignored) on this Mac's home-network address
only, to private-network clients only, with byte ranges for iPhone video
playback. It follows the address when it changes (checked every 5 s);
`install` adds a login item (`~/Library/LaunchAgents/dev.lightable.review.plist`)
that starts it at login and restarts it if it stops. Every
`tools/demo.mjs` run ends by printing the current URL, checked (starting
the server if it is down), and keeps the Mac from sleeping while it runs
(`caffeinate`; a closed lid still sleeps it). The page shows the newest run first (contact sheets, check
results, videos with subtitles, each with its time and note) and older
runs folded below; it reloads itself when new output appears.

## Publishing (only after approval)

```bash
node tools/demo.mjs approve --mode normal      # or --mode narration
```

copies the current final videos of that mode (never drafts), their
WebVTT captions and two posters into `web/demo/`; `tools/build_web.sh` then
builds lightable.dev/demo/ and lists it in `sitemap.xml`. Without approved
videos in `web/demo/` the demo page is not built.

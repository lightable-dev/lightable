# Voice-over for the demo

For the bracket example (Rev B → Rev C). Play it with `?demo&narration`
(add `&format=landscape` or `&format=vertical` for a 16:9 or 9:16 frame).
The steps and gestures are those of the normal demo
(`docs/demo/demo-script.md`); each step lasts long enough to say its line
at about 2.5 words a second, with half a second before the line and 0.7 s
after it, and never less than the normal step. No captions are drawn; the
voice and the subtitle files carry the words. Everything on screen still
comes from the comparison, and every action is a real input.

The lines live in `crates/lightable/src/demo.rs` (`VOICEOVER`), where the
timing is computed; the test `the_voiceover_matches_the_comparison` checks
every fact below against the comparison of the two example files.

## Script

| Step | Line | On screen |
|---|---|---|
| Opening | "Rev C of this mounting bracket just came in. What changed?" | Rev C open on its own |
| Drop | "Drop both revisions on the page. Nothing to install, and nothing is uploaded." | The pointer drags the two files in and drops them; the names appear |
| Overlay | "Lightable finds ten changes. Blue is added, red deleted, yellow changed." | Overlay, change list, the legend highlighted |
| Cloud | "The revision cloud marks one of them, the bolt note. In this example, the other nine weren't clouded." | Pinch onto the cloud: M16 readable |
| Changes | "Every change is in plain words: the holes grew from 13.5 to 17.5 millimetres, and the edge dimensions from 20 to 25. Tap one to zoom to it." | Tap a hole in the list; at "and the edge…" tap the 20 → 25 mm dimension (the list scrolled first in 9:16) |
| Side by side | "See before and after, side by side." | Click Side by side |
| Report | "Then share a review report." | Click Details, then Export PNG report; the report on screen |
| Phone | "It works on your phone too." | The comparison in a phone outline |
| End card | "Lightable. Free, open source, and your files are read in your browser, never uploaded. lightable.dev" | End card |

## Fact check against the comparison

| Statement | Comparison | Result |
|---|---|---|
| "this mounting bracket", "Rev C" | Title block: STEEL MOUNTING BRACKET, REV C | Correct |
| "finds ten changes" | 10 design changes (plus 8 revision-markup items, listed separately) | Correct |
| "Blue is added, red deleted, yellow changed" | Legend colours and words: added (blue), deleted (red), changed (yellow) | Correct |
| "marks one of them, the bolt note" | 1 design change inside the cloud: the BOLTS note, M12 → M16 | Correct |
| "In this example, the other nine weren't clouded" | 10 − 1; the example was drawn with only the bolt note clouded (`cloud_by_design`) | Correct, and said of this example only |
| "the holes grew from 13.5 to 17.5 millimetres" | 4 circles, each "diameter changed from 13.5 mm to 17.5 mm" | Correct |
| "the edge dimensions from 20 to 25" | Two dimensions "changed from 20 mm to 25 mm" | Correct |
| "Tap one to zoom to it" | A click on a row selects it and zooms there | Correct |
| "share a review report" | Details, then Export PNG report (also JSON) | Correct (two clicks, not one) |
| "your files are read in your browser, never uploaded" | The page reads files in the browser and sends nothing (the recorder checks the network) | Correct |

## Cue sheet (narration video)

Times from the first frame of `demo-*-narration.mp4`. Record each line,
then drop the clip at its start time; each slot is the line at 2.5 words a
second, so a clip can run a little long into the gap before the next.

| # | Step | Start | End | Line |
|---|---|---|---|---|
| 1 | Opening | 00:00:00.500 | 00:00:04.900 | Rev C of this mounting bracket just came in. What changed? |
| 2 | Drop | 00:00:06.100 | 00:00:11.300 | Drop both revisions on the page. Nothing to install, and nothing is uploaded. |
| 3 | Overlay | 00:00:12.500 | 00:00:16.900 | Lightable finds ten changes. Blue is added, red deleted, yellow changed. |
| 4 | Cloud | 00:00:18.100 | 00:00:25.300 | The revision cloud marks one of them, the bolt note. In this example, the other nine weren't clouded. |
| 5 | Changes | 00:00:26.500 | 00:00:37.700 | Every change is in plain words: the holes grew from 13.5 to 17.5 millimetres, and the edge dimensions from 20 to 25. Tap one to zoom to it. |
| 6 | Side by side | 00:00:38.900 | 00:00:41.700 | See before and after, side by side. |
| 7 | Report | 00:00:42.900 | 00:00:44.900 | Then share a review report. |
| 8 | Phone | 00:00:46.900 | 00:00:49.300 | It works on your phone too. |
| 9 | End card | 00:00:50.900 | 00:00:55.700 | Lightable. Free, open source, and your files are read in your browser, never uploaded. lightable.dev |

Video length 00:00:56.400 (the end card is held after the last line).

The same cues are in `out/demo/demo-narration.srt` and `.vtt` (made by
`lightable demo-cues out/demo`).

## Use-case videos (the demo page)

One voice-over line per step, for the branded videos (tools/stage/). Each
video tells hook, action, reveal, proof, payoff, end card; the captions
stay in the picture in every version, and the voice says the same facts.
Record each line within its times (at most 2.8 words a second; the first
line starts after the 1.2 s sting). The videos come with a quiet music bed
and UI sounds, without music, and as separate stems
(`out/demo/audio/*-music.wav`, `*-sfx.wav`), so a recorded voice can be laid
on its own track. Made by `lightable demo-cues out/demo`
(`feature-ID-voice.md`, and `feature-ID-voice.srt` as a guide to record to);
a test keeps the lines here equal to the plans.

### 1. The changes the cloud leaves out (gearbox cover)

| # | Scene | Start | End | On screen | Say |
|---|---|---|---|---|---|
| 1 | hook | 00:00:01.300 | 00:00:05.304 | Rev C's cloud marks just 1 change. Is that all? | Rev C's cloud marks just 1 change. Is that all? |
| 2 | action | 00:00:05.704 | 00:00:08.967 | Let's drop in Rev B and Rev C. | Let's drop in Rev B and Rev C. |
| 3 | reveal | 00:00:09.267 | 00:00:13.967 | It finds 14 changes between Rev B and Rev C. | It finds 14 changes between Rev B and Rev C. |
| 4 | proof | 00:00:14.267 | 00:00:19.667 | Only 1 is clouded. The other 13 aren't. | Only 1 is clouded. The other 13 aren't. |
| 5 | proof | 00:00:19.967 | 00:00:25.100 | This hole grew from 11 mm to 13.5 mm. | This hole grew from 11 mm to 13.5 mm. |
| 6 | proof | 00:00:25.400 | 00:00:30.533 | On layer SECTION, a line moved 1.25 mm up. | On layer SECTION, a line moved 1.25 mm up. |
| 7 | payoff | 00:00:30.833 | 00:00:34.267 | Every change is in the list, clouded or not. | Every change is in the list, clouded or not. |

Video length 00:00:37.367 with the stage's 1.2 s sting; the end card is held after the last line.

### 2. Tiny changes, exact values (locating plate)

| # | Scene | Start | End | On screen | Say |
|---|---|---|---|---|---|
| 1 | hook | 00:00:01.300 | 00:00:04.193 | Some changes are too small to see. | Some changes are too small to see. |
| 2 | action | 00:00:04.593 | 00:00:07.856 | Let's drop in Rev B and Rev C. | Let's drop in Rev B and Rev C. |
| 3 | reveal | 00:00:08.156 | 00:00:12.856 | It finds 3 changes between Rev B and Rev C. | It finds 3 changes between Rev B and Rev C. |
| 4 | proof | 00:00:13.156 | 00:00:17.956 | This Ø8 hole moved 0.5 mm right. | This Ø8 hole moved 0.5 mm right. |
| 5 | proof | 00:00:18.356 | 00:00:24.289 | Up close, you can see the 0.5 mm shift. | Up close, you can see the 0.5 mm shift. |
| 6 | proof | 00:00:24.589 | 00:00:28.689 | Details give the exact value: 0.5 mm right. | Details give the exact value: 0.5 mm right. |
| 7 | proof | 00:00:28.989 | 00:00:33.789 | The tolerance tightened from ±0.1 to ±0.05. | The tolerance tightened from ±0.1 to ±0.05. |
| 8 | payoff | 00:00:34.089 | 00:00:36.189 | Small changes, exact values. | Small changes, exact values. |

Video length 00:00:39.289 with the stage's 1.2 s sting; the end card is held after the last line.

### 3. Re-export noise ignored (drive shaft, simulated re-export)

| # | Scene | Start | End | On screen | Say |
|---|---|---|---|---|---|
| 1 | hook | 00:00:01.300 | 00:00:06.600 | Rev E is a simulated re-export of Rev D. | Rev E is a simulated re-export of Rev D. So what really changed? |
| 2 | action | 00:00:07.000 | 00:00:10.263 | Let's drop in Rev D and Rev E. | Let's drop in Rev D and Rev E. |
| 3 | reveal | 00:00:10.563 | 00:00:15.263 | It finds just 1 change. | It finds just 1 change. Renumbered, reordered, older DXF: not changes. |
| 4 | proof | 00:00:15.563 | 00:00:20.696 | The one real change: Ra 1.6 became Ra 0.8. | The one real change: Ra 1.6 became Ra 0.8. |
| 5 | proof | 00:00:20.996 | 00:00:25.096 | 17 objects were just redrawn, not changed. | 17 objects were just redrawn, not changed. |
| 6 | payoff | 00:00:25.396 | 00:00:27.719 | Real changes only, not file noise. | Real changes only, not file noise. |

Video length 00:00:30.819 with the stage's 1.2 s sting; the end card is held after the last line.

### 4. One block edit, every door (apartment plan)

| # | Scene | Start | End | On screen | Say |
|---|---|---|---|---|---|
| 1 | hook | 00:00:01.300 | 00:00:04.563 | Edit a block once, and every copy changes. | Edit a block once, and every copy changes. |
| 2 | action | 00:00:04.963 | 00:00:08.226 | Let's drop in Rev A and Rev B. | Let's drop in Rev A and Rev B. |
| 3 | reveal | 00:00:08.526 | 00:00:13.226 | It finds 4 changes between Rev A and Rev B. | It finds 4 changes between Rev A and Rev B. |
| 4 | proof | 00:00:13.526 | 00:00:19.030 | The door block changed, and all 7 doors show it. | The door block changed, and all 7 doors show it. |
| 5 | proof | 00:00:19.330 | 00:00:24.463 | On layer WALLS, a line moved 300 mm right. | On layer WALLS, a line moved 300 mm right. |
| 6 | proof | 00:00:24.763 | 00:00:28.863 | Here's before and after, side by side. | Here's before and after, side by side. |
| 7 | payoff | 00:00:29.163 | 00:00:31.856 | One block edit, shown everywhere it's used. | One block edit, shown everywhere it's used. |

Video length 00:00:34.956 with the stage's 1.2 s sting; the end card is held after the last line.

### 5. Big drawings, compared in your browser (warehouse layout)

| # | Scene | Start | End | On screen | Say |
|---|---|---|---|---|---|
| 1 | hook | 00:00:01.300 | 00:00:03.822 | A big warehouse layout. What changed? | A big warehouse layout. What changed? |
| 2 | action | 00:00:04.222 | 00:00:07.856 | Drop both in. It all runs in this browser. | Drop both in. It all runs in this browser. |
| 3 | reveal | 00:00:08.156 | 00:00:12.856 | It finds 3 changes among 23,182 objects. | It finds 3 changes among 23,182 objects. |
| 4 | proof | 00:00:13.156 | 00:00:16.256 | Read and compared in a few seconds. | Read and compared in a few seconds. |
| 5 | proof | 00:00:16.556 | 00:00:19.656 | Since the drop, the browser logged [as shown on screen] requests. | Since the drop, the browser logged [as shown on screen] requests. |
| 6 | proof | 00:00:19.956 | 00:00:24.756 | This text changed from R1-065 to QC-01. | This text changed from R1-065 to QC-01. |
| 7 | payoff | 00:00:25.056 | 00:00:27.748 | Big drawings, compared right in your browser. | Big drawings, compared right in your browser. |

Video length 00:00:30.848 with the stage's 1.2 s sting; the end card is held after the last line.

The large layout's request count is measured as the video plays: say the number on screen.

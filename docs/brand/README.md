# Lightable brand kit

One kit for every video and page: the five use-case videos use all of it,
the same way. Made by `tools/gen_brand.py` (the SVGs; Lato is copied in
unmodified). The stage that applies it to the videos is `tools/stage/`.

## Logo

| File | Use |
|---|---|
| `logo-mark.svg` | The mark alone (icons, the intro sting). |
| `wordmark.svg` | "lightable" in Lato Regular, outlined (no font needed). |
| `lockup.svg` | Mark and wordmark together (end card, video footer). |

The mark is a light table: two sheets laid over each other, the earlier
revision in blue, the later in yellow, and where they overlap the light
shines through in white. It is the app's own idea (compare by overlaying)
in the app's own colours. Keep it on dark backgrounds; leave clear space of
half the mark's width around it; never recolour, rotate or outline it.

## Colours

The app's change colours, on a light-table night.

| Name | Hex | Use |
|---|---|---|
| Ink | `#0b0f14` | Backgrounds, text on light cards. |
| Night | `#101924` | Background gradient top. |
| Deep | `#143049` | The glow under the window. |
| Paper | `#eef2f6` | Text, file cards. |
| Mist | `#9aa7b4` | Secondary text. |
| Signal blue | `#4cb0ff` | Brand accent; the app's "added". Pointer ripples, the drop's landing pulse, "lightable.dev". |
| Marker yellow | `#f5d02a` | The app's "changed". Key numbers in captions; the pulse around a highlighted change. |
| Red | `#f04848` | The app's "deleted". Never decorative. |
| Grey | `#6a6a6a` | The app's "unchanged". |

## Type

**Lato** (Regular 400 and Light 300, version 1.105, © Łukasz Dziedzic),
under the SIL Open Font Licence 1.1: `fonts/` with `fonts/OFL.txt`. Used
for titles and captions in the videos only; the app keeps its own fonts.
Lato has no "→": the stage draws the arrow as a small stroke icon.

## Captions

- Said as a person would say them ("This hole grew from 11 mm to 13.5
  mm."), never UI labels; every number from the comparison.
- Lato Regular, at least 4 % of the frame's height (4.5 % when it fits),
  at most two lines, balanced; a number stays with its unit or noun, "Rev
  B" and short phrases ("from 11 mm") stay together.
- Paper on an ink pill (95 % opaque), radius 0.32 em, always in the same
  place (where the app keeps the drawing clear, above the frame's bottom
  15 %); the frame's top-left 10 % holds no control.
- One highlight colour, used once: only the scene's key number in marker
  yellow (the step names it).
- In: a plain fade (0.3 s), as the step's action starts; on screen at
  least as long as it takes to read (three words a second, 2 s at least).
- Only one set of captions: in the picture. A player's subtitle track is
  offered only for the narration versions (no captions in the picture).

## Scenes

- One idea per scene. While a scene's point is on screen nothing else
  moves: the pointer has done its work, the camera is still.
- **Spotlight:** once the step's action is done, the app around what the
  step is about (the change, the zoom's subject, the highlighted line) is
  softly dimmed; it lifts just before the next step.
- **Holds:** the result ("It finds 14 changes...") at least 5 s; each proof
  holds still after its action for its caption's reading time, 3 s at
  least.
- **Step chips** in the window's title bar: 1 Drop · 2 Result · 3 Proof,
  the current one in signal blue.
- The window fills most of the frame (912 × 506 of 960 × 540; 524 × 924 of
  540 × 960).

## Motion

- **Easing:** out-cubic for things arriving, in-out-cubic for the camera.
- **Pointer:** eased by the recorder, a 0.3 s pause on a control; lifts 12 %
  with a shadow while hovering, dips while pressed; a blue ripple (0.45 s)
  where it presses.
- **Camera:** the window settles in after the sting (96 % → 100 %), pushes
  in 2 % over 0.8 s as the result comes, eases back over 0.8 s as the
  payoff starts; still in between. The app's own zooms stay the app's.
- **Transitions:** sting → window: fade and scale (0.33 s). Window → end
  card: fade (0.42 s), the lines rising in one after another.

## Intro sting (1.2 s)

The two sheets slide in from either side (0.42 s), the overlap lights up
(0.33 s in), the wordmark rises in (0.42 s in), and the whole fades and
grows slightly into the window (from 0.87 s). A soft three-note chime.

## End card (3 s)

The lockup, then "Free. Open source.", "Your files are read in your
browser, never uploaded." and "lightable.dev" in signal blue: the app's own
end-card words, three lines rising in. A warm four-note chord.

## Sound

Our own (`tools/gen_sound.py`): a quiet bed (Cmaj7, Am7, Fmaj7, G6 at 84
bpm), and UI sounds for the sting, the pickup, the drop, a click, the
reveal and the end card, placed where the recording really did each. Every
video comes with music and without, plus separate stems for a voice-over.

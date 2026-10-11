# First-viewer test and known limits

The "understood within 5 seconds" goal cannot be checked by the people who
built the demo. Run it with first-time viewers, ideally mechanical or
manufacturing engineers who review supplier drawings, and some who do not.

## How to run it

- Open https://lightable.dev (or the local build) on a normal connection,
  and once on a slow one (Chrome DevTools: Network → "Slow 4G", which is the
  old "Fast 3G"). Do not explain anything first.
- Let the viewer look for 5 seconds, then hide the screen and ask the
  questions below. Write down their words, not a score.
- Then play `?demo` once, without commentary, and ask again.

## What to ask after 5 seconds

1. What is this page for? (Looking for: comparing two revisions of a
   drawing.)
2. How many things changed between the two revisions? (10, plus revision
   markup. Do they say 1 because of the cloud, or 10?)
3. Which change would worry you before sending this to a supplier? (Any
   of the hole size / hole position / dimension changes.)
4. Where would you click to see one of the changes?
5. Do you think your drawing would be uploaded somewhere if you used it?

## What to ask after the demo

6. In one sentence, what did Lightable do that you would otherwise do by
   hand?
7. Did anything in the demo claim more than you believe the tool did? (We
   want to hear if anyone thinks it checks design correctness,
   manufacturability or what the revision cloud should cover. It does not.)
8. Was any caption too fast to read, or unclear?
9. Would you trust the list enough to stop comparing the PDFs yourself?
   Why or why not?

## What to watch

- Whether the eye goes to the change list or the drawing first.
- Whether "10 changes" next to one revision cloud registers as a
  contradiction (the intended point) or as noise.
- Whether the colours (yellow changed, green added) are read without the
  legend.
- On the slow connection: whether the static preview and list are read as
  the app (and clicked) before the app has loaded. They are not
  interactive until it has.
- Phone width: the preview stacks above the list; the app itself is not
  designed for phones.

## Known limits

- Lightable compares drawing data, not plots. It does not check design
  correctness, manufacturability, tolerances, standards compliance, or what
  a revision cloud should cover.
- "The revision cloud marks 1 change" is the demo's narration about this
  example (centre of a change inside the cloud), not a product feature.
- "Not compared yet" lists what is skipped: text and dimension styles,
  other tables, other header settings, objects, and the dimension pictures
  in anonymous blocks (the dimension entities themselves are compared).
- Dimension values are compared by measurement, the geometric distance:
  a dimension style's length scale factor (DIMLFAC) is not applied, so in
  such a drawing the headline's number differs from the one drawn. An
  overridden dimension text is compared as text, not interpreted.
- The example is an illustration: a plausible engineering change with
  cited references, not a design calculation. The bolt load, the mating
  frame and the 5 mm move are assumptions of the example.
- The web app needs WebGL2 and downloads about 1.6 MB (compressed) before it
  is interactive: about 12 s on "Slow 4G" / "Fast 3G". The preview and list
  appear within about 1.3 s.
- Symbols that only egui's emoji fonts had (☑ ✔ ★ ⚠ …) are drawn as a
  replacement glyph in drawing text.
- Safari has not been tested (no automation access on this machine).

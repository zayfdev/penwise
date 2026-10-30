---
name: penwise
description: Use when the owner reviews a design on the Penwise board — design review, "check my marks", "look at what I drew", "fix this" about a page, a sent round ("Penwise: round N sent"), variants to pick from, or any visual change to a project linked with `penwise link`.
---

# Penwise design review

The owner draws marks with a pen on a live copy of this project (the **board**). You read the marks, propose a
change on the board, edit the code after the owner picks, and close the round. The owner reads the terminal,
not the canvas: report files and lines you changed there.

## Tools (the project comes from `PENWISE_PROJECT`; none takes a project argument)

| Tool | Use it to |
|---|---|
| `look` | Read the marks of the latest sent round (else the open one): text plus one numbered overview image per frame. `{mark: id}` for one full bundle with crops, `{frame: id}` to filter, `images: "none"` for text only. |
| `show` | Put exactly one thing on the board: `frame {route, preset?, extent?}`, `ghost {frame, patch}` (one temporary patch), `variants {frame, options:[{label, patch}]}` (2-4), `sketch {strokes, near?}`, `image {path}`, `focus {frame \| page}`. `clear: true` removes a ghost or variants. |
| `resolve` | Say what you did: `marks:[{id, status: done \| reopened \| seen}]`, and `round: n` to close round n. |
| `notes` | Read the owner's plain notes (not the design board): list pages, handwriting as text, `query`, `png`. |

## The loop (about four calls)

1. `look` when the owner says to, or when the prompt starts with `Penwise: round N sent`. Read the text first:
   each line has the mark's number, id, status, kind, colour meaning, handwriting, selector, `file:line`
   sources and key styles. The overview image shows the same numbers. `since send` marks belong to the next round.
2. Say in one sentence what you think each mark asks. A mark without an element (`@x,y WxH`) is an area: say so
   rather than guess an element.
3. Propose before editing: `show ghost` for one option, `show variants` for 2-4 labelled options. A patch is
   `{css, dom:[{op, selector, ...}]}`: CSS and simple DOM ops only, no JavaScript. Read `failed` in the answer:
   a selector that matched nothing.
4. `look` again: the `picks:` section names the variant the owner chose. **Never edit code before the owner
   picks** (or says to go ahead), and never edit files that no mark and no pick asked for.
5. Edit the code at the `file:line` sources. Live reload or HMR refreshes the frame.
6. `resolve {marks: [...done], round: n}` with the **round number from `look`'s header** (`round 3 sent`).
   Closing a round freezes the frame with the owner's ink into history, moves the live frame right, and clears
   ghosts and variants. An open (unsent) round needs its number too; `round: true` only means the latest sent one.
   Set a mark `reopened` only if the owner says the change was wrong.

## Mark vocabulary

| Mark | Meaning |
|---|---|
| Circle / loop | Look here (focus) |
| Arrow | Move this there: tail = what, head = where (`→` in the text names the head's element) |
| Cross (X) | Remove |
| Wavy line under | Feels off: ask or propose |
| Box drawn in empty space | New thing here; its contents are the sketch |
| Line through text | Change this text; handwriting nearby is the new text |
| Anything else, handwriting | Read freely |

Colours: **red** = problem, **blue** = idea, **green** = good, keep. The kind is a hint with a confidence,
not a rule: read the handwriting and the image too. A `reopened` mark means the owner is asking again.

## Board model

- One infinite board per project. A **frame** is a live page at a preset (`desktop` 1440x900 default,
  `tablet`, `phone`); the default extent is `full`: the box is the whole document, no scrolling inside it.
- Lanes: one row per screen (route + preset), lanes stack downward; time runs left to right inside a lane.
- Left of the live frame: history pictures of resolved rounds. The live frame is "now". Right: variants.
- `show frame` reuses the frame already showing that route and preset, and moves the owner's view to it.
- Marks follow their element (bound) or their document position (area), and show only at their own preset.

## Rules

- Propose, wait for the pick, then edit. One question at a time; if two marks conflict, ask which wins.
- Quote ids from `look` exactly; ids are 32 hex characters.
- If `look` says `element` is gone or a mark has no selector, say so instead of inventing one.
- Tool errors come back as text with the server's reason (for example `409` when a round has no marks): read
  it and adapt; do not retry blindly.
- Report in the terminal: files and lines changed, and which marks you set `done`.

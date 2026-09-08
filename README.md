# vellum

An offline D&D Beyond character sheet for the ClockworkPi uConsole.

Fetch your character once. Everything after that is local, keyboard-driven and
works with the wifi off.

```
vellum fetch https://www.dndbeyond.com/characters/147474826
vellum                # interactive sheet
```

```
vellum tui [id]            interactive sheet
vellum show [id] [flags]   one static frame, scriptable, no network
vellum fetch <id|url>      pull and cache a character
```

## Why it works this way

D&D Beyond has no public API and has had none for years. What it does have is a
read-only JSON view of any character whose privacy is set to Public:

| endpoint | anonymous |
|---|---|
| `www.dndbeyond.com/character/{id}/json` | **200 — full payload** |
| `character-service.dndbeyond.com/character/v5/character/{id}` | 403 |

The `character-service` endpoint every VTT importer uses is closed to anonymous
callers. The plain `/json` route is open, needs no credentials, no session
cookie and no token — the share link's trailing token is decorative, the bare id
is enough.

So `vellum` makes **one GET per invocation**, initiated by you, and caches the
result. That is a browser-shaped access pattern. Don't turn it into a poller.

The payload embeds all rules text — spell descriptions, item text, class
features — which is why one fetch is genuinely enough to run a whole session
offline.

## The part that is actually work

D&D Beyond ships **no derived values**. Not AC, not proficiency bonus, not skill
modifiers, not even spell slot maxima. The raw payload has ability scores and a
list of modifiers; every number a player reads off a sheet is computed from
those. `src/derive/` is that computation, and it is the product — the UI is a
view over it.

Three rules that are easy to get wrong and are pinned by tests:

- **`stats` is not what D&D Beyond displays.** Feat and racial bonuses arrive as
  separate `bonus … -score` modifiers. Miss them and every derived number is off.
- **`isGranted: false` does not mean inactive.** It means *chosen by the player*
  rather than granted automatically. Expertise and most skill proficiencies come
  through as `false`. Filtering on it silently halves the sheet.
- **`set-base` takes the maximum, never the sum.** Two darkvision entries of 60
  and 120 mean 120ft, not 180ft.
- **`#[serde(default)]` does not cover an explicit `null`.** D&D Beyond nulls
  fields that look like they never could be — `alwaysPrepared: null` on a spell
  failed the entire import. Every scalar goes through a `nullable`
  deserialiser so a stray null degrades to the default.

Modifiers carrying a `restriction` are never folded into a number. They are
surfaced in their own section, because they are the ones you have to make a
ruling about at the table.

Every derived value carries the formula that produced it (`Leather 11 + DEX +3`),
so a mismatch against your live sheet tells you which rule is wrong.

## Layout

```
src/
├── ddb/
│   ├── schema.rs   narrow serde structs — model only what we read
│   └── fetch.rs    the only code that talks to the network
├── derive/
│   ├── tables.rs   5e constants (SRD 5.2.1, CC-BY-4.0)
│   └── mod.rs      payload -> Sheet.  the product.
├── app/
│   ├── state.rs    interaction state — no ratatui, no terminal, fully testable
│   ├── keys.rs     the keymap, likewise headless
│   ├── ui.rs       ratatui drawing
│   └── theme.rs    amber phosphor as styles
├── content.rs      payload -> uniform tab rows, + HTML-to-text
├── tabs.rs         the 22-row tabbed layout and detail overlay
├── portrait.rs     avatar -> amber half-blocks
├── session.rs      mutable play state, its own file, never the snapshot
├── device.rs       uConsole panel simulation (80x22) + amber palette
├── render.rs       flat sheet; phase 1's acceptance test
└── main.rs         fetch / show / path

scripts/
└── uconsole        run inside a uConsole-shaped terminal
```

`serde` ignores unknown fields and every struct is `#[serde(default)]`, so D&D
Beyond can add or restructure anything we don't read without breaking us. That
is a far smaller surface than the DOM scraping that keeps breaking Beyond20.

## Navigating the sheet

Seven tabs, each budgeted against the 22 rows the panel actually has:

```
row  1-2   status   name/class, then HP/AC/INIT/PB/PP — always visible
row  3     rule
row  4     tab strip
row  5     rule
rows 6-20  content   15 rows. the entire working budget.
row  21    rule
row  22    keybinds + "12-26 of 34"
```

```
RIHANNE KAHM "DELLEN NURR" · Elf Rogue 8 (Assassin)
HP 49/55 [█████████░]  AC 14  INIT +3  PB +3  PP 15
────────────────────────────────────────────────────
 1 VITALS  2 SKILLS [3·ACTIONS] 4 SPELLS  5 GEAR ...
────────────────────────────────────────────────────
Cunning Action        Bonus Action   On your turn,…
Uncanny Dodge         Reaction       When an attac…
────────────────────────────────────────────────────
1-7 tab  j/k move  ↵ detail  / find  q quit  10 items
```

**The status bar never moves.** HP and AC are what you glance at mid-combat, so
they are never a tab away — that is asserted by a test, not just intended.

**Digits jump, they do not cycle.** `3` is ACTIONS from anywhere. Cycling with
Tab through six panes to reach the one you want is fine at a desk and wrong
three seconds into your turn. Tab/Shift-Tab still cycle for browsing.

**Every content tab is the same list.** Name, a short meta chip, a one-line
snippet. `j`/`k` move, `Enter` opens the row full-screen with the complete rules
text, `Esc` returns, `/` filters. One set of keybindings across all of them,
because the rows are a uniform type.

**Master-detail, not scrolling.** Fifteen rows holds fifteen list entries, and
rules text runs 400-500 characters. So the list stays a list and the prose opens
over the top of it. All of that text is already in the payload — the detail view
never touches the network.

| tab | contents |
|---|---|
| VITALS | portrait, ability scores, saves, senses, class notes |
| SKILLS | all 18, proficient first, expertise marked `**` |
| ACTIONS | class/race/feat actions with action-economy chips |
| SPELLS | every bucket merged, cantrips first, deduplicated |
| GEAR | inventory + custom items, `worn` / `attuned` / `x3` |
| FEATS | feats, class features filtered to your level, racial traits |
| NOTES | background, personality, bonds, flaws, backstory |

### Keys

```
navigation
  1-7            jump straight to a tab      tab/shift-tab  cycle
  j/k  ↑/↓       move the selection          g/G            top/bottom
  space/pgdn     page                        enter          open full-screen
  /              filter this tab             esc            back one layer
  n/p            next/prev row without leaving the detail view

play
  d              damage      type a number, enter to apply
  h              heal        "
  t              temp hp     "
  c              conditions overlay          i    inspiration
  r              rest — short or long
  s / f          death save success / failure  (only while dying)

  q  ctrl-c      quit
```

Play keys work from every tab. Mid-combat you should not have to navigate
somewhere before you can take damage.

`esc` backs out one layer at a time — detail, then the filter, then quit —
rather than dumping you out of the app from three levels deep.

While filtering, every printable key is text. You can search for `javelin`
without `1` jumping tabs or `q` quitting mid-word.

`show` renders any one frame statically, which is useful for scripting and for
diffing layouts:

```
vellum show --tab spells
vellum show --tab feats --scroll 15
vellum show --tab actions --detail 6
```

## Play state

Everything that changes during a session lives in its own file,
`<id>.session.json`, and the snapshot is never written to. Re-importing after a
level-up replaces the snapshot and leaves your hit points alone.

It stores **damage taken**, not current hit points. A level-up raises max HP in
the snapshot; storing damage means you stay wounded by the same amount rather
than being mysteriously healed by levelling.

Written after every change rather than on quit — a uConsole running off two
18650s can lose power mid-session, and re-entering an hour of combat tracking is
worse than a few milliseconds of IO. A save that fails says so in the header.

A first launch seeds from whatever the snapshot last recorded, so it agrees with
the website instead of starting you at full health. A session belonging to a
different character, or one that will not parse, is ignored and reseeded — the
sheet is the thing you need at the table, and a bad file must never block it.

`vellum reset` clears play state and leaves the snapshot and portrait alone.

The 5e rules that are easy to get wrong, all pinned by tests:

- damage comes off temporary hit points first, and temp pools replace rather
  than stack
- hit points never go below zero
- **damage taken at zero is a failed death save** — the one people forget
- dropping to zero clears prior death-save progress and applies Unconscious
- any healing above zero clears death saves and wakes you
- a long rest removes **one** level of exhaustion, not all of them

Conditions and a zero-hit-point total are the only things allowed to break the
amber palette. They render red, which is exactly why they read instantly.

## The portrait

`vellum fetch` caches the avatar next to the snapshot, so `show` stays offline.
It renders as amber phosphor using U+2580 UPPER HALF BLOCK — foreground is the
top pixel, background the bottom — which gives two near-square pixels per cell.
At 20x7 cells that is a 20x14 image.

Colour is deliberately discarded. A full-colour photo in the middle of an amber
sheet looks like a mistake; luminance mapped onto the amber ramp looks like a
CRT. `--ascii` switches to a `@%#*+=-:.` ramp for terminals without truecolour,
at half the vertical resolution.

D&D Beyond only serves the 150x150 thumbnail — 300/400/600 all return 403 — but
the cell count is the real limit, so a larger source would not help much. At
this size the portrait reads as a silhouette, not a likeness. That is the
aesthetic, not a defect.

## Testing

100 tests, and the interaction model is the point of the architecture: `app/state.rs`
and `app/keys.rs` depend on neither ratatui nor a terminal, so every key a player
can press is exercised headlessly — selection memory across tabs, filter scoping,
the escape ladder, clamping when a filter shrinks the list under the cursor.

`tests/render_frames.rs` draws real frames through ratatui's `TestBackend` at
the panel's exact 80x22, asserting that HP and AC survive on every tab and that
an empty filter says so rather than drawing a blank pane. It also draws at
20x6 and 200x60, because a dev laptop window is not a uConsole.

```
cargo test
cargo clippy --all-targets
```

## Developing without the hardware

The uConsole's panel is 1280x720. At ~294 PPI a 1x bitmap font is unreadable
across a table, so the realistic setting is an 8x16 face at 2x — which is
**exactly 80 columns by 22 rows**. Twenty-two rows is the binding constraint on
every layout decision in this project.

```
scripts/uconsole            # amber, bezel, page 1
scripts/uconsole --page 2   # next screenful
scripts/uconsole --3x       # the 53x15 grid you get at 3x scaling
scripts/uconsole --raw      # resize your real terminal to 80x22 and paint it
```

`--raw` asks the terminal emulator to resize itself via the xterm escape
(iTerm2 and Terminal.app both honour it) and restores your previous size on
exit. That is the closest local approximation available.

Everything else draws a bezel around a simulated panel, so you can work in a
normal-sized terminal and still see exactly what the device will show:

```
┌ uConsole · 1280x720 @ 8x16 font, 2x · 80x22 · page 1/2 ───────┐
│ ...exactly 22 rows of exactly 80 columns...                   │
└───────────────────────────────────────────────────────────────┘
! 38 lines of content, 22 rows of panel — 2 screens, 16 still below
```

The warnings are the point. A harness that says "fits" when it does not just
moves the discovery to the table, so `tests/device.rs` pins the paging and
clipping maths.

The current sheet is 38 lines and needs two screens at 2x, and does not fit at
all at 3x. That is what makes the phase 2 TUI tabbed rather than scrolling.

`cool-retro-term` is the real target for CRT curvature and phosphor glow, and it
installs from apt on the device. Its macOS cask was disabled in September 2026
for failing Gatekeeper, so the amber palette here is emitted directly as
truecolor ANSI instead.

## Status

- [x] **Phase 0** — fetch + cache
- [x] **Phase 1** — derive layer + tests
- [x] **Phase 1.5** — portrait, seven tabs, detail view, device harness
- [x] **Phase 2** — ratatui TUI: tabs, list navigation, detail overlay, live
      filtering, amber theme, portrait
- [x] **Phase 3** — play state: hit points, temp hp, conditions, exhaustion,
      death saves, rests, persisted separately from the snapshot
- [ ] **Phase 4** — d20 roller wired to the row you are on
- [ ] **Phase 3** — session layer: HP, conditions, death saves, local overrides
The snapshot is immutable and session state lives in a separate file, so
re-importing after a level-up never clobbers HP you are tracking mid-combat.

## Building for the uConsole

```
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu
```

Building on-device works but a cold build is 15+ minutes on a CM4. Run it inside
`cool-retro-term` with an amber profile — the CRT curvature and phosphor glow
come free and hardware-accelerated, and at the uConsole's ~294 PPI a bitmap font
at 2x is razor sharp.

## Content

Rules constants in `derive/tables.rs` are SRD 5.2.1 (CC-BY-4.0). Character
snapshots are **not** — they embed non-SRD rules text and must never be
committed. See `tests/fixtures/README.md`.

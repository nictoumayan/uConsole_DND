# vellum

An offline D&D Beyond character sheet for the ClockworkPi uConsole.

Fetch your character once. Everything after that is local, keyboard-driven and
works with the wifi off.

```
vellum                # opens the load screen on a first run
vellum fetch https://www.dndbeyond.com/characters/123456789
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
├── rules.rs        2024 mechanics: conditions, exhaustion, advantage resolution
├── dice.rs         PCG32, dice expressions, advantage
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
| VITALS | portrait, scores and saves, then speed, senses, defences, languages |
| SKILLS | all 18, proficient first, expertise marked `**` |
| ROLL | initiative, 6 saves, 18 skills, death save — all rollable |
| ACTIONS | class/race/feat actions with action-economy chips |
| SPELLS | every bucket merged, cantrips first, deduplicated |
| GEAR | inventory + custom items, `worn` / `attuned` / `x3` |
| FEATS | feats, class features filtered to your level, racial traits, proficiencies |
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
  c              conditions overlay (shows what each one does)
  i              inspiration
  o              correct ability scores by hand
  L              load a character by URL
  u / U          spend / restore one limited use
  r              rest — short or long
                 short opens a screen: space spends one hit die at a time
  s / f          death save success / failure  (only while dying)

dice
  enter          roll the selected row       (any row that rolls)
  D              roll the selected attack's damage
  a / z          roll it with advantage / disadvantage
  x              free-form dice: 2d6+3, 1d20-1, 4d6
  l              roll log

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

## Limited uses

Anything with charges — a once-per-rest feat, a lineage spell, a wand — shows
pips in the list. `u` spends one, `U` hands one back, for the misclick and for
a DM who rules that one did not count.

D&D Beyond is not consistent about this: actions and spells carry a **numeric**
`resetType`, inventory items carry a **string** (`"Dawn"`). Both shapes have to
deserialise or one wand fails the entire import, so `ResetType` is an untagged
enum. Codes 1 and 2 are corroborated by this character's own data — the feat
whose text says "Once per Short/Long Rest" carries 1, the lineage spells that
recharge overnight carry 2 — and anything else renders as "special" rather than
being guessed at. A wrong recharge label is worse than no label.

An entry with `maxNumberConsumed` but no `maxUses` is an upcastable spell, not
a charge, and gets no tracker.

A short rest restores only what recharges on one; a long rest restores
everything. The recharge type is stored **alongside** the count in the session
rather than looked up from the snapshot, so a rest works from the session alone
and a re-import can never orphan the bookkeeping.

## The rules engine

`src/rules.rs` holds the 2024 mechanics that turn a sheet into rolls. It is
deliberately separate from `derive`: `derive` answers "what are this
character's numbers", `rules` answers "what happens when they roll". Keeping
them apart is what stops the sheet and the dice disagreeing.

**The advantage you ask for is a request, not an instruction.** Pressing `a`
adds one source; the engine also folds in the character's standing advantages,
every condition in play, and exhaustion, then applies the cancellation rule:

> If circumstances cause a roll to have both Advantage and Disadvantage, the
> roll has neither of them, and you roll one d20.

So being Poisoned and pressing `a` correctly produces a **straight roll**, and
the footer says `cancelled by Poisoned`. That is the case people get wrong at a
table, and it is the reason this module exists.

Standing advantages come from the sheet without being asked. A Rogue's class
grants advantage on Initiative, so rolling it shows `adv: initiative` with no
keypress.

What it models:

- **Exhaustion**: -2 on every d20 test per level and -5 ft of Speed per level,
  applied to attacks, checks, saves, initiative and death saves alike
- **All 15 conditions**, with their real effects on attacks, ability checks and
  specific saving throws, including automatic failures
- **Initiative is a Dexterity check** in the 2024 rules, so anything that
  hampers ability checks hampers initiative — easy to miss by hand
- **Speed** after exhaustion, and zero while Grappled, Restrained, Paralyzed,
  Stunned, Unconscious or Petrified
- **Massive damage**: damage that zeroes you with a remainder at or above your
  hit point maximum kills outright, no death saves
- **Unarmored Defense** for Barbarian and Monk, with the shield rule
- **Spell save DC** and spell attack bonus
- Carrying capacity, and whether you are incapacitated

Verified line by line against the official **SRD 5.2.1 PDF** rather than a
secondary source, which turned up four things summaries had got wrong or
omitted:

- **Incapacitated gives Disadvantage on Initiative** specifically. It does
  nothing to ability checks in general, so this is invisible unless you read
  the entry.
- **Invisible gives Advantage on Initiative** for the same reason.
- **Stunned does not zero your Speed.** Grappled, Restrained, Paralyzed,
  Petrified and Unconscious each carry an explicit "Speed 0" clause. Stunned
  does not — it only incapacitates. This was wrong in the first cut.
- **Passive Perception shifts by 5**, not 0, when you have Advantage or
  Disadvantage on Perception checks. Being Poisoned drops a passive 15 to 10.

What it deliberately does **not** decide: whether you can see the source of
your fear, whether the attacker is within five feet, whether a check relies on
sight. Those surface as a note in the conditions overlay so you make the
ruling, rather than the app pretending to.

## Attacks

Equipped weapons become rollable attacks at the top of the ACTIONS tab, and
none of it is parsed out of prose. The payload carries the damage dice, the
damage type, the range and the weapon properties as structured fields, and a
feature like Sneak Attack carries dice **already scaled to your level** — the
`{{scalevalue}}` in its description is resolved in the `dice` field.

`↵` rolls to hit, `D` rolls damage. The attack roll goes through the rules
engine like any other d20 test, so being Poisoned gives it Disadvantage without
being told; damage does not, because conditions and exhaustion modify d20 tests
and a damage roll is not one.

Finesse takes the better of Strength and Dexterity, every time — "your choice"
means the better one. Correcting an ability score moves every attack with it,
which is the reason attacks derive from the sheet rather than from the payload.

**`↵` does the obvious thing for the row, not the tab.** A row that rolls,
rolls; anything else opens. Sneak Attack has no attack roll of its own, so
enter opens it and `D` still rolls its 4d6. `v` opens any row.

## Loading a character

`L` opens a load screen: paste a URL or an id, press enter. Every shape works —
a share link with its trailing token, a plain character URL, or the bare number.

The fetch blocks for a second or two, so the keymap only validates and hands the
id to the event loop, which fetches *after* the frame saying "fetching…" has
been painted. A URL is text while you are typing it: no digit jumps a tab and
no letter fires a command.

Running `vellum` with nothing cached opens this screen rather than printing
usage at someone who just wants their sheet, and escape will not strand you
there with an empty sheet and no way back.

The command line and the load screen share one loader, so the two paths cannot
drift into caching different things.

## Rests and hit dice

One Hit Point Die per class level, of that class's size, tracked per die size so
a multiclass character keeps its 5d8 and its 3d10 apart.

A **short rest** is not one keystroke. The rules say "you can decide to spend an
additional Hit Point Die after each roll", so it opens a screen: `space` spends
one die, rolls it, adds Constitution, and heals — **minimum 1**, which matters
for a character whose Constitution modifier is negative. The running total and
the last roll stay on screen so you can decide whether to spend another. Short
rest features recharge on entry.

A **long rest** restores all hit points and **all** spent hit dice. That is a
2024 change worth stating plainly: the 2014 rules gave back half, which is the
version most people remember, and it is what this would have shipped with had
the rule not been read from the source.

Both rests require at least 1 hit point to start, and say so rather than
silently doing nothing.

## Dice

The ROLL tab lists everything a DM asks you for — initiative, saves, skills,
death saves — as the same uniform list every other tab uses. So `/` filters it,
and **`3` `/` `ste` `enter` `enter`** is the whole path from anywhere in the app
to a rolled Stealth check. `a` and `z` roll the same row with advantage or
disadvantage. `x` rolls free-form dice from any tab.

Modifiers come from the derive layer, so a roll can never disagree with the
sheet it is printed next to.

The most recent roll stays on the status bar with its breakdown — at a table you
read the result out, get asked "with what modifier?", and read it out again.
`l` opens the full log; naturals 20 and 1 are coloured, because they are the two
results everyone reacts to.

**Death saves apply themselves.** Rolling one and then recording it by hand is
double-entry that gets skipped mid-fight, so a roll on that row also updates the
tally: a natural 20 brings you back at one hit point, a natural 1 counts as two
failures, and 10 or better is a success.

### On the randomness

PCG32, seeded from the clock mixed with an allocation address, in about twenty
lines and with no dependency. Range reduction uses rejection sampling rather
than `%`, because modulo bias on a d20 is exactly the kind of unfair that
players notice over a campaign. It is seedable, so every roll in the test suite
is deterministic, and `tests/dice.rs` runs a chi-square over 200,000 d20 rolls
plus a check that advantage and disadvantage land on their theoretical means of
13.825 and 7.175.

## What D&D Beyond does not ship

Some things the website's own sheet applies are **absent from the character
data entirely**. There is nothing to compute from, so the number has to come
from you: press `o`, correct the score, and armour class, passive perception,
every save, every skill and initiative all follow.

The known case is the **2024 background ability increases**. A background
grants +2/+1 or +1/+1/+1 across three listed abilities — the Scribe background
covers Dexterity, Intelligence and Wisdom. D&D Beyond applies them. The
character JSON contains no trace: `"wisdom-score"` appears **zero times** in a
200KB payload, and the feat that represents the increase carries no modifiers.

This was caught by comparing the derived sheet against the live one: armour
class and passive perception were each low by exactly 1, hit points and both
senses were correct. Correcting Dexterity to 18, Intelligence and Wisdom to 16
reconciled every value at once — which is the tell that the root cause is the
scores and not the formulas above them.

The correction is deliberately made on the **score**, never on the derived
value. One entry fixes everything downstream and stays correct as the character
levels; six patches on six derived numbers would not.

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

A terminal cell carries two colours. Half blocks spend that on two vertical
pixels; **quadrant** blocks spend it on four, by picking the glyph whose filled
quadrants match which of a 2x2 subgrid are the brighter of the cell's own two
levels. That doubles horizontal resolution for nothing, and it works here
precisely because the image is monochrome — two levels per cell is a real
constraint on a colour photo and almost none on an amber one.

At 24x12 cells that is a 48x24 image, three times the detail of the half-block
version it replaced. The split point is the midpoint of each cell's own range
rather than a global threshold, so a dark cell keeps its internal detail
instead of collapsing to black, and a flat cell renders as solid background
rather than a full block whose two colours happen to match.

Twenty-four wide by twelve tall renders square: a subcell is about twice as
tall as it is wide, so `cols == rows * 2`.

Colour is deliberately discarded. A full-colour photo in the middle of an amber
sheet looks like a mistake; luminance mapped onto the amber ramp looks like a
CRT. `--ascii` switches to a `@%#*+=-:.` ramp for terminals without truecolour,
at half the vertical resolution.

D&D Beyond only serves the 150x150 thumbnail — 300/400/600 all return 403 — but
the cell count is the real limit, so a larger source would not help much. At
this size the portrait reads as a silhouette, not a likeness. That is the
aesthetic, not a defect.

## Testing

207 tests, and the interaction model is the point of the architecture: `app/state.rs`
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
- [x] **Phase 4** — dice: the ROLL tab, advantage, free-form expressions, a
      roll log, and self-applying death saves
- [x] **Limited uses** — charges on actions, spells and items, restored by the
      right kind of rest
- [x] **Rules engine** — conditions, exhaustion, advantage resolution, speed,
      unarmored defense, spell DC, massive damage
- [x] **Hit dice and rests** — per-pool tracking, an interactive short rest,
      a long rest that restores the whole pool
- [x] **Attacks** — weapon attacks and damage from structured payload data
- [x] **Load by URL** — paste a character link into the app
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

## Writing changes back to D&D Beyond

Not built, and not straightforward. Google OAuth is not a path — Google is the
identity provider *to* D&D Beyond, and there is no third-party OAuth flow at
D&D Beyond to grant an application access to your characters. The only write
path is reverse-engineered, authenticated with the browser session cookie.

The analysis, the verified auth mechanism, a design that would work, and the
risks are in [docs/sync.md](docs/sync.md).

## Content

Rules constants in `derive/tables.rs` and the mechanics in `rules.rs` are
SRD 5.2.1 (CC-BY-4.0), Wizards of the Coast, taken from the official PDF at
<https://www.dndbeyond.com/srd>. Character
snapshots are **not** — they embed non-SRD rules text and must never be
committed. See `tests/fixtures/README.md`.

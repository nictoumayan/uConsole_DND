# Development queue

Ordered by what would actually bite at a table, not by what is interesting to
build. Everything below is a proposal; nothing here is committed to.

---

## 0. Run it on the hardware

**Not a feature, and it blocks judging everything else.**

Every frame in this project has been verified against a simulated panel. The
binary has never run on an ARM Linux device, in a real terminal, on a battery.
Three specific things could be wrong and none of them are visible from here:

- **Glyph coverage.** The portrait uses quadrant blocks — `▘▝▖▗▚▞▙▟▛▜` — which
  are rarer than the half blocks they replaced. A console font without them
  renders a face as a grid of boxes. The pip characters `●○` and the box drawing
  have the same exposure.
- **Truecolour.** The amber palette is 24-bit RGB. A terminal that only speaks
  256 colours will approximate it, and the phosphor look is the whole point.
- **Geometry.** 80×22 is arithmetic from the panel spec. What the framebuffer
  console actually reports may differ, and the layout has no slack.

Steps:

1. `brew install rustup`, then `rustup target add aarch64-unknown-linux-gnu`,
   or `cargo install cross` and build through Docker.
2. Cross-compile release, copy the binary to the device.
3. Verify in this order: it starts, the geometry is 80×22, box drawing renders,
   quadrant glyphs render, the amber is amber.
4. Measure two numbers that matter: cold start time, and battery drain over a
   session-length run.

The device harness was built so this step would find *nothing* new. Whether it
does is the actual test of that bet.

---

## 1. Stop polling for input — DONE

**Half an hour. Done first because it was free.**

The event loop polls every 250 milliseconds:

```rust
if !event::poll(Duration::from_millis(250))? { continue; }
```

That is four wakeups a second, forever, doing nothing, on a device with a
three-hour battery and a four-hour session. The comment above it claims polling
is needed to catch resizes — that is wrong. `event::read()` blocks until
something happens and returns `Event::Resize` like any other event.

Replaced with a blocking read. Nothing in the app needs a periodic tick, so
there is no timeout to keep.

---

## 2. Undo — DONE

**Half a day.**

Eleven places mutate session state and none of them can be taken back. Typing
`d 30` when you meant `d 3` is recoverable by healing 27 back. Toggling the
wrong condition, spending the wrong limited use, or a mis-keyed death save is
not — and all three happen mid-combat, which is exactly when you are least able
to reason about repairing state by hand.

Shipped as thirty levels rather than one, on `Ctrl-Z`. The snapshot is taken
around the whole keystroke rather than inside each mutating method, so a
mutation added later is covered without anyone remembering to cover it, and the
label is derived by comparing the two sessions rather than recorded by hand.

The open question resolved as expected: undo leaves the roll log alone. The
dice really came up what they came up.

---

## 3. Spell slots and Concentration

**The largest functional gap. Three or four evenings.**

Any spellcaster is currently unusable. The Rogue this was built against has no
slots, which is why it never surfaced.

The payload does not help much: `spellSlots` arrives as
`{"level": 1, "used": 0, "available": 0}` — the maxima are zero, exactly like
armour class and proficiency bonus. They have to come from class tables.

- **3a. Slot tables.** Full casters (Bard, Cleric, Druid, Sorcerer, Wizard),
  half casters (Paladin, Ranger), third casters (Eldritch Knight, Arcane
  Trickster), and the multiclass rule that combines them at different weights.
  Pact Magic is a separate pool with its own progression.
- **3b. Derive maxima** per slot level from those tables.
- **3c. Session tracks slots used**, keyed by level, restored by a long rest —
  and Pact Magic by a *short* rest, which is the distinction people forget.
- **3d. SPELLS tab**: slot pips in the header, cast a spell at a chosen level
  and expend the slot. Upcasting means the slot level and the spell level differ.
- **3e. Concentration.** Track which spell is being concentrated on. Then the
  feature this app is uniquely placed to provide: **on taking damage, prompt the
  Constitution save automatically, with the DC already computed.**

  > The DC equals 10 or half the damage taken (round down), whichever number is
  > higher.

  The app knows the damage — it just took it. Everyone forgets this save, and
  nobody wants to do the arithmetic mid-fight. Starting a second Concentration
  spell also ends the first, which the app can enforce rather than trusting you
  to remember.

- **3f. Tests against a real caster payload**, not a synthetic one.

---

## 4. A caster and a multiclass fixture

**One evening. Do it alongside 3, not after.**

Everything is tested against one single-class Rogue. That leaves whole branches
of the derive layer unexercised: Unarmored Defense (Barbarian, Monk), spell save
DC, Pact Magic, and hit dice pools that actually differ.

Two more SRD-only fixtures — a caster and a multiclass character — would exercise
them. The multiclass one matters most: `derive_hit_dice` and the attack code
both have multiclass branches that no test has ever run.

---

## 5. Level-up and re-import

**Half a day.**

The whole architecture was built for this — the snapshot is immutable, session
state lives separately, and damage is stored rather than current hit points
precisely so a level-up leaves you wounded by the same amount. **None of that
has ever been exercised.**

What needs deciding and testing when a re-import changes the character:

- maximum hit points rise — damage should carry, which the design intends
- a class level is added — hit dice pools grow; spent dice should carry
- a limited-use maximum changes from 2 to 3 — uses spent should carry
- an ability score changes on the website — does a local correction still apply,
  or is it now wrong and actively harmful?

That last one is the real question. An ability override that was compensating
for a missing background bonus becomes wrong the moment the website's own number
changes. The app should probably notice and say so.

---

## 6. Table conveniences

**Each an evening or less. Pick by what annoys you first.**

- **Notes.** Somewhere to type during play. The NOTES tab is read-only; a
  scratch pad that persists in the session is the obvious addition.
- **Currency.** Spend and earn gold. Structured in the payload, unused today.
- **Session export.** Write the roll log and what changed to a file, for a
  player who likes a record.
- **Inspiration is a toggle** but the 2024 rules call it Heroic Inspiration and
  let you reroll — worth a line of help text rather than a bare star.

---

## Not planned

- **An initiative or encounter tracker.** That is the DM's job and a different
  application.
- **Writing changes back to D&D Beyond.** See [sync.md](sync.md).
- **Homebrew or character editing.** Edit on the website, re-import. Building an
  editor means reimplementing D&D Beyond's builder, badly.
- **Spell or rules content beyond the payload.** Everything needed is already
  embedded in the character data; a separate compendium is a different project.

---

## Suggested order

```
0 ──────────────────────────► hardware  (blocks judging anything else)
   └─ 1 polling               free, do it in the same sitting

        2 undo                small, high value at a table

        4 fixtures ──┬──► 3 spell slots + concentration
                     └──► 5 level-up / re-import

                          6 conveniences, as they annoy you
```

**0 and 1 together** — one sitting, and the hardware run tells you whether the
rest of the queue is even aimed correctly.

**2 next** because it is small and every session benefits.

**4 before 3**, not after: writing the caster fixture first means the spell slot
work has something to test against from its first line, rather than being
verified afterwards.

**3 is the big one** and should be broken across evenings by sub-task. 3a and 3b
are pure functions with tables — the same shape as the derive layer, and just as
testable headlessly. 3e is the part worth building the rest for.

**5 is the sleeper.** It is the least visible item and the one most likely to
lose real data, because the design promised something it has never been made to
prove.

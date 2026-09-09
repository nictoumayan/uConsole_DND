# vellum

An offline D&D Beyond character sheet for the ClockworkPi uConsole. Rust, a
ratatui TUI, no runtime and no network after the first fetch.

Read `README.md` for what it does and `docs/roadmap.md` for what is next.
This file is for what would otherwise be learned the hard way.

---

## The three decisions everything else follows from

**1. The snapshot is immutable; play state is a separate file.**
`<id>.snapshot.json` is what D&D Beyond sent and is never written to.
`<id>.session.json` holds everything that changes during play. Re-importing
after a level-up replaces the snapshot and leaves hit points alone. The session
stores **damage taken**, not current hit points, so a level-up leaves you
wounded by the same amount rather than mysteriously healed.

**2. `derive` answers "what are this character's numbers"; `rules` answers
"what happens when they roll".** Keeping them apart is what stops the sheet and
the dice disagreeing. Anything that depends on conditions or exhaustion belongs
in `rules`; anything computed once from the payload belongs in `derive`.

**3. The interaction model does not depend on a terminal.** `app/state.rs` and
`app/keys.rs` import neither ratatui nor crossterm's terminal. Every key a
player can press is exercised headlessly. Keep it that way — it is why the
whole app can be developed without the hardware.

---

## D&D Beyond's payload will mislead you

There is no public API. `www.dndbeyond.com/character/{id}/json` serves a public
character anonymously; `character-service.dndbeyond.com` 403s. One GET per
invocation, initiated by the player — do not turn it into a poller.

Verified traps, each one already a named test:

- **It ships no derived values.** No armour class, no proficiency bonus, no
  skill modifiers, not even spell slot maxima. `derive/` computes all of it.
- **`stats` is not what the website displays.** Feat and racial score bonuses
  arrive as separate `bonus … -score` modifiers.
- **`isGranted: false` does not mean inactive.** It means *chosen by the player*
  rather than granted automatically. Expertise and most skill proficiencies
  come through as `false`.
- **`set-base` takes the maximum, never the sum.** Darkvision 60 and 120 is
  120ft, not 180ft.
- **`#[serde(default)]` does not cover an explicit `null`.** The payload nulls
  fields that look like they never could be. Every scalar goes through the
  `nullable` deserialiser in `ddb/schema.rs`.
- **The same field changes type.** `resetType` is an integer on actions and
  spells, a string on inventory items. Hence the untagged `ResetType` enum.
- **Some things the website applies are absent entirely.** The 2024 background
  ability increases are the known case: `"wisdom-score"` appears zero times in
  a 200KB payload. That is why `Session::ability_overrides` exists. When a
  derived number disagrees with the live sheet, suspect missing input before
  suspecting the formula.

---

## Rules come from the PDF, not from memory

The official SRD 5.2.1 (CC-BY-4.0) is at <https://www.dndbeyond.com/srd>.
**Check it before encoding a rule.** Two were wrong from memory and both were
caught only by reading it:

- A 2024 long rest restores **all** spent hit dice. 2014 restored half.
- Multiclass caster levels round half-caster levels **up**. 2014 rounded down.

Two more were wrong from secondary sources: Stunned does not zero your Speed,
and Passive Perception shifts by 5 with advantage or disadvantage.

Where the SRD omits something — Eldritch Knight and Arcane Trickster are not in
it — say so in the code rather than implying it was verified.

---

## The device is the design constraint

1280×720 at ~294 PPI, with an 8×16 bitmap font at 2×, is **exactly 80 columns
by 22 rows**. That is asserted, not commented. Twenty-two rows is why the sheet
is tabbed rather than scrolling, and why the status bar grows a third row only
when it has something to say.

`scripts/uconsole` runs the app at that geometry. `src/device.rs` simulates the
panel with a bezel and reports anything that does not fit — a harness that says
"fits" when it does not is worse than none.

The app has never run on the real hardware. Glyph coverage for the quadrant
blocks in the portrait, truecolour support, and the console's real geometry are
all unverified. See roadmap item 0.

---

## Working agreement

- **`cargo test` and `cargo clippy --all-targets` before declaring done.**
  Clippy is kept at zero warnings, not "warnings are fine".
- **Name tests after the behaviour, not the function.**
  `damage_taken_at_zero_is_a_failed_death_save`, not `test_take_damage`.
  A failing name should tell you what broke without opening the file.
- **Comment the *why*, especially where the code looks wrong.** Half the
  comments here exist because the obvious implementation was the wrong one.
- **Never commit a real character snapshot.** It embeds non-SRD rules text.
  Fixtures in `tests/fixtures/` are hand-authored and SRD-only;
  `tests/fixtures/real/` is gitignored.
- **Prefer structured payload fields over parsing prose.** Attacks looked like
  they needed `{{scalevalue}}` parsing and did not — the dice are in `dice`,
  already scaled.
- **Ask before adding a dependency.** The tree is `anyhow`, `directories`,
  `image` (decode only), `ratatui`, `serde`, `serde_json`, `ureq`. Dice are
  PCG32 in twenty lines rather than a crate, so rolls are seedable and every
  test is deterministic.

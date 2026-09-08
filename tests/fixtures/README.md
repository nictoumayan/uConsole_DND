# Fixtures

`srd_rogue.json` is **hand-authored** and contains no D&D Beyond content — no
spell text, no item descriptions, no class feature prose. It is shaped like a
real payload and carries only mechanical values, which are game rules, not
copyrightable expression.

**Never commit a real character snapshot here.** The live payload embeds full
rules text for every spell, item and feature the character has, and outside the
SRD that text belongs to Wizards. Caching it on your own device is fine;
putting it in a public repo is not.

To run the derive tests against your own character instead, drop the snapshot
at `tests/fixtures/real/<id>.json` — that path is gitignored, and
`real_character_matches_dndbeyond` picks it up automatically and skips when
it is absent.

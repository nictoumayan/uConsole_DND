# Writing changes back to D&D Beyond

**Status: not built. Pinned for later.**
Findings below were verified on 2026-09-09 against the live service. None of it
is documented or stable, so re-verify before acting on any of it.

`vellum` is read-only today. It fetches a character once and every change you
make — hit points, conditions, death saves, limited uses — stays in
`<id>.session.json` on the device. This note is about what it would take to
push those changes back, and why the obvious approach does not work.

---

## Google OAuth is not a path, and the reason matters

The instinct is reasonable: the account logs in with Google, so use Google
OAuth. It does not work, for a structural reason rather than a missing feature.

Google is the **identity provider to D&D Beyond**. A Google OAuth token proves
who you are *to Google*. D&D Beyond has no endpoint that accepts a Google token
as authorization to read or modify a character — the federation is entirely
between you and their login page.

What would be needed is the other thing: an OAuth flow *at D&D Beyond*, where
you grant a third-party application scoped access to your characters. There is
no such flow. There is no "Sign in with D&D Beyond" for developers, no
registered-application concept, and no public API of any kind.

This has been asked for since 2018. D&D Beyond staff have said the blocker is
licensing Wizards' IP rather than engineering effort. It has sat on the
"planned, longer term" roadmap for years and there is still nothing in 2026.

So: **there is no sanctioned write path.** Everything below is
reverse-engineered.

---

## What actually exists

The credential is the `CobaltSession` cookie from a logged-in browser — the
same one DDB Importer and every VTT tool uses. It exchanges for a short-lived
bearer token:

```
POST https://auth-service.dndbeyond.com/v1/cobalt-token

  no cookie      → 200  {"token": null, "ttl": 1800}
  bogus cookie   → 401  {"token": null, "ttl": 1800}
  real cookie    → 200  {"token": "<jwt>", "ttl": 1800}
```

The bearer is good for **30 minutes** and authorizes `character-service` calls.

For contrast, the read paths this project already relies on:

| endpoint | anonymous |
|---|---|
| `www.dndbeyond.com/character/{id}/json` | 200, full payload (public characters) |
| `character-service.dndbeyond.com/character/v5/character/{id}` | 403 |

The read route `vellum` uses needs no credential at all. Writing would be the
first time this project holds one.

### The unknown

The write endpoints themselves. A guessed `PUT /character/v5/life/hp` returns
404, so the real paths have to be discovered by watching the browser's network
tab while changing hit points on the actual sheet.

This is a solved problem elsewhere — there is at least one MCP server claiming
hit point, spell slot, death save and currency writes against these endpoints —
so the shape is known to work. It is discovery work, not research.

---

## The design, if it gets built

### Manual push, never continuous sync

A `p` key, or `vellum push`. Continuous sync turns one authenticated request
per session into hundreds, which is the difference between a browser-shaped
access pattern and something that looks like a bot.

### Push only what maps one-to-one

| session field | D&D Beyond field |
|---|---|
| `damage` | `removedHitPoints` |
| `temporary_hp` | `temporaryHitPoints` |
| `conditions` | `conditions` |
| `death_successes` / `death_failures` | `deathSaves` |
| `inspiration` | `inspiration` |
| `uses` | per-entity `limitedUse.numberUsed` |

Never push derived values. `vellum` computes AC, proficiency bonus and skill
modifiers locally precisely because D&D Beyond does not store them; pushing a
computed number into a field that is meant to hold raw data would corrupt the
character.

### Pull, verify, then push

Re-fetch the character first and compare. If D&D Beyond has changed underneath
you — you levelled up on the website, or played a session on the phone — refuse
and say so rather than clobbering it. Same instinct as a shrink guard on a
backup: the failure mode worth designing against is silently destroying the
good copy.

### Credential handling

- The cookie is entered on the device and stored device-local, mode `0600`.
- Never in the repo, never in a commit, never logged, never in an error message.
- Exchange for the 30-minute bearer on demand; do not persist the bearer.
- The cookie expires. Failing to refresh must degrade to "push unavailable",
  never to a hang or a crash mid-session.

### Keep it quarantined

A separate module behind a cargo feature flag. The read-only core stays clean,
the reverse-engineered part is opt-in, and anyone reading the repo can see
exactly where the line is.

---

## The risks, stated plainly

**Terms of service.** D&D Beyond prohibits accessing the service with "agents,
robots, scripts, or spiders" without written permission. Reading your own
public character once per session is browser-shaped and defensible. Writing is
a different posture: authenticated, state-mutating, and unambiguously automated
access. The practical risk to one person updating their own character is low
but not zero, and it is an account-level risk, not a technical one.

**Churn.** These endpoints are undocumented and change without notice. Beyond20
has spent years in a reactive break-fix cycle for exactly this reason. Read
support survives that better than write support, because a broken read fails
visibly and a broken write can fail silently or write the wrong thing.

**Blast radius.** A read bug shows you a wrong number. A write bug corrupts the
character on the website. That asymmetry is the strongest argument for
pull-verify-push and for a manual trigger.

---

## The cheaper alternative

If the actual goal is "my DM and my party can see my current hit points", the
official D&D Beyond mobile app already syncs, and it has offline character
sheets besides.

The uConsole stays the authority at the table; the phone stays the sync path;
no reverse-engineered code gets written at all. Less satisfying, zero risk, and
worth ruling out deliberately rather than by omission.

---

## If picked up

1. Capture the real write endpoints from the browser network tab (change HP,
   toggle a condition, spend a limited use — one request each).
2. Confirm the bearer exchange end to end with a real cookie.
3. Build `push` behind a feature flag, read-only dry-run first: print the diff
   it *would* send and send nothing.
4. Only then wire the actual requests, one field at a time, starting with
   `removedHitPoints` because it is the easiest to verify by eye.

//! Phase 0: pull the character payload once, cache it, never touch the network
//! again until the player asks.
//!
//! This is the only code in the project that talks to D&D Beyond, and it makes
//! exactly one GET per invocation. There is no official API; this is the
//! public read-only JSON view of a character whose privacy is set to Public.
//! Keep it that way — one request, initiated by the player, is a browser-shaped
//! access pattern. Polling it would not be.

use anyhow::{bail, Context, Result};
use std::time::Duration;

const UA: &str = concat!("vellum/", env!("CARGO_PKG_VERSION"), " (personal character sheet)");

/// Accepts a bare id, or any of the URL shapes D&D Beyond hands out:
///   123456789
///   https://www.dndbeyond.com/characters/123456789
///   https://www.dndbeyond.com/characters/123456789/AbCdEf   <- share link
///
/// The trailing share token is decorative as far as the JSON route is
/// concerned; the bare id returns the full payload for a public character.
pub fn parse_character_id(input: &str) -> Result<i64> {
    let trimmed = input.trim().trim_end_matches('/');
    if let Ok(id) = trimmed.parse::<i64>() {
        return Ok(id);
    }
    let after = trimmed
        .split("/characters/")
        .nth(1)
        .or_else(|| trimmed.split("/character/").nth(1))
        .with_context(|| format!("could not find a character id in {input:?}"))?;
    let id_part = after.split('/').next().unwrap_or_default();
    id_part
        .parse::<i64>()
        .with_context(|| format!("{id_part:?} is not a character id"))
}

pub fn character_url(id: i64) -> String {
    format!("https://www.dndbeyond.com/character/{id}/json")
}

/// Fetch the raw payload. Returned as a String rather than parsed so the
/// caller can write the exact bytes to disk — the snapshot on disk should be
/// what D&D Beyond sent, not our re-serialisation of it.
pub fn fetch_raw(id: i64) -> Result<String> {
    let url = character_url(id);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .user_agent(UA)
        .build();

    match agent.get(&url).call() {
        Ok(resp) => resp.into_string().context("reading response body"),
        Err(ureq::Error::Status(403, _)) => bail!(
            "403 from D&D Beyond for character {id}.\n\
             The character's privacy is probably not set to Public — open it on \
             dndbeyond.com and set privacy to Public, or the JSON route will \
             refuse anonymous readers."
        ),
        Err(ureq::Error::Status(404, _)) => bail!(
            "404 — no character {id}. Check the id in your share link."
        ),
        Err(ureq::Error::Status(code, _)) => bail!("D&D Beyond returned HTTP {code}"),
        Err(e) => Err(anyhow::Error::new(e).context("network error talking to D&D Beyond")),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_character_id;

    #[test]
    fn parses_every_url_shape() {
        let cases = [
            "123456789",
            "  123456789  ",
            "https://www.dndbeyond.com/characters/123456789",
            "https://www.dndbeyond.com/characters/123456789/",
            "https://www.dndbeyond.com/characters/123456789/AbCdEf",
            "https://www.dndbeyond.com/character/123456789/json",
        ];
        for c in cases {
            assert_eq!(parse_character_id(c).unwrap(), 123456789, "failed on {c:?}");
        }
    }

    #[test]
    fn rejects_junk() {
        assert!(parse_character_id("https://example.com/nope").is_err());
        assert!(parse_character_id("").is_err());
    }
}

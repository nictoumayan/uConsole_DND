//! Dice.
//!
//! No RNG dependency: PCG32 is about twenty lines, is statistically far better
//! than anything a dice roller needs, and being seedable means every roll in
//! the test suite is deterministic. Range reduction uses rejection sampling
//! rather than `%`, because modulo bias on a d20 is exactly the kind of unfair
//! that players notice over a campaign.

use std::fmt;

const MULT: u64 = 6_364_136_223_846_793_005;

pub struct Rng {
    state: u64,
    inc: u64,
}

impl Rng {
    pub fn from_seed(seed: u64) -> Rng {
        let mut r = Rng { state: 0, inc: (seed << 1) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    pub fn from_entropy() -> Rng {
        // Nanoseconds since the epoch, mixed with the address of a heap
        // allocation so two processes started in the same nanosecond diverge.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5EED);
        let boxed = Box::new(0u8);
        let addr = &*boxed as *const u8 as u64;
        Rng::from_seed(nanos ^ addr.rotate_left(17))
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULT).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in `0..n`, rejecting the biased tail rather than taking `%`.
    fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let zone = u32::MAX - (u32::MAX % n) - 1;
        loop {
            let v = self.next_u32();
            if v <= zone {
                return v % n;
            }
        }
    }

    pub fn die(&mut self, sides: u32) -> u32 {
        self.below(sides) + 1
    }
}

impl Default for Rng {
    fn default() -> Self {
        Rng::from_entropy()
    }
}

/// How a d20 is rolled. Anything that is not a d20 check ignores this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Advantage {
    #[default]
    Normal,
    Advantage,
    Disadvantage,
}

impl Advantage {
    pub fn label(self) -> &'static str {
        match self {
            Advantage::Normal => "",
            Advantage::Advantage => "adv",
            Advantage::Disadvantage => "dis",
        }
    }
}

/// `2d6+3`, `1d20-1`, `4d6`, or a bare `d20`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expr {
    pub count: u32,
    pub sides: u32,
    pub modifier: i32,
}

impl Expr {
    pub fn d20(modifier: i32) -> Expr {
        Expr { count: 1, sides: 20, modifier }
    }

    /// Deliberately strict. A typo that silently rolls something other than
    /// what you typed is worse than being told it is a typo.
    pub fn parse(text: &str) -> Result<Expr, String> {
        let s: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let s = s.to_ascii_lowercase();
        if s.is_empty() {
            return Err("nothing to roll".into());
        }

        let (dice, modifier) = match s.find(['+', '-']) {
            Some(i) => {
                let (a, b) = s.split_at(i);
                let m: i32 = b.parse().map_err(|_| format!("bad modifier {b:?}"))?;
                (a.to_string(), m)
            }
            None => (s.clone(), 0),
        };

        let Some(d) = dice.find('d') else {
            return Err(format!("{text:?} has no 'd' — try 1d20+5"));
        };
        let (count_s, sides_s) = dice.split_at(d);
        let sides_s = &sides_s[1..];

        let count: u32 = if count_s.is_empty() {
            1
        } else {
            count_s.parse().map_err(|_| format!("bad dice count {count_s:?}"))?
        };
        let sides: u32 = sides_s.parse().map_err(|_| format!("bad die size {sides_s:?}"))?;

        if count == 0 || count > 100 {
            return Err("dice count must be 1-100".into());
        }
        if !(2..=1000).contains(&sides) {
            return Err("die size must be 2-1000".into());
        }
        Ok(Expr { count, sides, modifier })
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}d{}", self.count, self.sides)?;
        match self.modifier.cmp(&0) {
            std::cmp::Ordering::Greater => write!(f, "+{}", self.modifier),
            std::cmp::Ordering::Less => write!(f, "{}", self.modifier),
            std::cmp::Ordering::Equal => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Roll {
    pub label: String,
    pub expr: Expr,
    pub advantage: Advantage,
    /// Every die face rolled, including the one advantage discarded.
    pub dice: Vec<u32>,
    /// The die actually used, once advantage has picked.
    pub kept: u32,
    pub total: i32,
}

impl Roll {
    pub fn is_d20(&self) -> bool {
        self.expr.sides == 20 && self.expr.count == 1
    }

    pub fn is_nat20(&self) -> bool {
        self.is_d20() && self.kept == 20
    }

    pub fn is_nat1(&self) -> bool {
        self.is_d20() && self.kept == 1
    }

    /// "d20 17 +9 = 26" — the breakdown, because at a table you read out the
    /// die face as well as the total.
    pub fn breakdown(&self) -> String {
        let faces = if self.dice.len() > 1 {
            let shown: Vec<String> = self
                .dice
                .iter()
                .map(|d| {
                    if *d == self.kept && self.advantage != Advantage::Normal {
                        format!("[{d}]")
                    } else {
                        d.to_string()
                    }
                })
                .collect();
            shown.join(" ")
        } else {
            self.kept.to_string()
        };

        let m = match self.expr.modifier.cmp(&0) {
            std::cmp::Ordering::Greater => format!(" +{}", self.expr.modifier),
            std::cmp::Ordering::Less => format!(" {}", self.expr.modifier),
            std::cmp::Ordering::Equal => String::new(),
        };
        // "d20 17 +9 = 26" for a single die, "2d6 1 5 +3 = 9" for several —
        // dropping the count made a two-dice roll read like a one-dice roll.
        let notation = if self.expr.count > 1 && self.advantage == Advantage::Normal {
            format!("{}d{}", self.expr.count, self.expr.sides)
        } else {
            format!("d{}", self.expr.sides)
        };
        format!("{notation} {faces}{m} = {}", self.total)
    }
}

pub fn roll(rng: &mut Rng, label: &str, expr: Expr, advantage: Advantage) -> Roll {
    // Advantage only means anything on a single d20; rolling 4d6 "with
    // advantage" is not a thing, so the flag is ignored there rather than
    // quietly doubling the dice.
    let single_d20 = expr.count == 1 && expr.sides == 20;
    let adv = if single_d20 { advantage } else { Advantage::Normal };

    let mut dice = Vec::new();
    let kept = match adv {
        Advantage::Normal => {
            for _ in 0..expr.count {
                dice.push(rng.die(expr.sides));
            }
            dice.iter().copied().sum::<u32>()
        }
        Advantage::Advantage | Advantage::Disadvantage => {
            let a = rng.die(20);
            let b = rng.die(20);
            dice.push(a);
            dice.push(b);
            if adv == Advantage::Advantage { a.max(b) } else { a.min(b) }
        }
    };

    Roll {
        label: label.to_string(),
        expr,
        advantage: adv,
        dice,
        kept,
        total: kept as i32 + expr.modifier,
    }
}

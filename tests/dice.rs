//! The dice have to be fair and the parser has to be strict. Players notice
//! bias over a campaign, and a typo that silently rolls the wrong thing is
//! worse than being told it is a typo.

use vellum::dice::{roll, Advantage, Expr, Rng};

#[test]
fn parses_the_shapes_people_actually_type() {
    assert_eq!(Expr::parse("1d20").unwrap(), Expr { count: 1, sides: 20, modifier: 0 });
    assert_eq!(Expr::parse("d20").unwrap(), Expr { count: 1, sides: 20, modifier: 0 });
    assert_eq!(Expr::parse("2d6+3").unwrap(), Expr { count: 2, sides: 6, modifier: 3 });
    assert_eq!(Expr::parse("1d20-1").unwrap(), Expr { count: 1, sides: 20, modifier: -1 });
    assert_eq!(Expr::parse("4d6").unwrap(), Expr { count: 4, sides: 6, modifier: 0 });
    assert_eq!(Expr::parse(" 2 D 8 + 2 ").unwrap(), Expr { count: 2, sides: 8, modifier: 2 });
}

#[test]
fn rejects_nonsense_rather_than_guessing() {
    for bad in ["", "hello", "20", "d", "0d6", "1d1", "101d6", "1d1001", "2d6+x"] {
        assert!(Expr::parse(bad).is_err(), "{bad:?} should not parse");
    }
}

#[test]
fn displays_the_way_it_was_typed() {
    assert_eq!(Expr::parse("2d6+3").unwrap().to_string(), "2d6+3");
    assert_eq!(Expr::parse("1d20-1").unwrap().to_string(), "1d20-1");
    assert_eq!(Expr::parse("4d6").unwrap().to_string(), "4d6");
}

#[test]
fn the_same_seed_gives_the_same_rolls() {
    // Determinism is what makes every other test here meaningful.
    let seq = |seed| {
        let mut r = Rng::from_seed(seed);
        (0..20).map(|_| r.die(20)).collect::<Vec<_>>()
    };
    assert_eq!(seq(42), seq(42));
    assert_ne!(seq(42), seq(43));
}

#[test]
fn every_face_is_in_range() {
    let mut r = Rng::from_seed(7);
    for sides in [2u32, 4, 6, 8, 10, 12, 20, 100] {
        for _ in 0..2_000 {
            let v = r.die(sides);
            assert!((1..=sides).contains(&v), "d{sides} produced {v}");
        }
    }
}

#[test]
fn a_d20_is_uniform() {
    // Rejection sampling exists precisely so this holds. With 200k rolls each
    // face expects 10,000; a chi-square over 19 degrees of freedom exceeds
    // ~43.8 only about one time in a thousand.
    let mut r = Rng::from_seed(0xD20);
    let n = 200_000;
    let mut counts = [0u32; 21];
    for _ in 0..n {
        counts[r.die(20) as usize] += 1;
    }
    let expected = n as f64 / 20.0;
    let chi2: f64 = (1..=20)
        .map(|f| {
            let d = counts[f] as f64 - expected;
            d * d / expected
        })
        .sum();
    assert!(chi2 < 43.8, "d20 looks biased: chi2 = {chi2:.1}, counts = {:?}", &counts[1..]);
}

#[test]
fn advantage_keeps_the_higher_die_and_disadvantage_the_lower() {
    let mut r = Rng::from_seed(99);
    for _ in 0..500 {
        let adv = roll(&mut r, "check", Expr::d20(0), Advantage::Advantage);
        assert_eq!(adv.dice.len(), 2, "advantage rolls two dice");
        assert_eq!(adv.kept, *adv.dice.iter().max().unwrap());

        let dis = roll(&mut r, "check", Expr::d20(0), Advantage::Disadvantage);
        assert_eq!(dis.kept, *dis.dice.iter().min().unwrap());
    }
}

#[test]
fn advantage_raises_the_average_and_disadvantage_lowers_it() {
    let mut r = Rng::from_seed(1234);
    let mean = |r: &mut Rng, a: Advantage| {
        let n = 20_000;
        let sum: i64 = (0..n).map(|_| roll(r, "", Expr::d20(0), a).total as i64).sum();
        sum as f64 / n as f64
    };
    let normal = mean(&mut r, Advantage::Normal);
    let adv = mean(&mut r, Advantage::Advantage);
    let dis = mean(&mut r, Advantage::Disadvantage);

    // Known values: 10.5 normal, 13.825 advantage, 7.175 disadvantage.
    assert!((normal - 10.5).abs() < 0.15, "normal mean {normal:.3}");
    assert!((adv - 13.825).abs() < 0.15, "advantage mean {adv:.3}");
    assert!((dis - 7.175).abs() < 0.15, "disadvantage mean {dis:.3}");
}

#[test]
fn advantage_is_ignored_on_anything_that_is_not_a_single_d20() {
    // "4d6 with advantage" is not a thing; it must not quietly roll 8 dice.
    let mut r = Rng::from_seed(5);
    let out = roll(&mut r, "damage", Expr::parse("4d6").unwrap(), Advantage::Advantage);
    assert_eq!(out.dice.len(), 4);
    assert_eq!(out.advantage, Advantage::Normal);
}

#[test]
fn a_modifier_is_added_once_not_per_die() {
    let mut r = Rng::from_seed(11);
    for _ in 0..200 {
        let out = roll(&mut r, "damage", Expr::parse("3d6+2").unwrap(), Advantage::Normal);
        let faces: u32 = out.dice.iter().sum();
        assert_eq!(out.total, faces as i32 + 2);
        assert!((5..=20).contains(&out.total), "3d6+2 out of range: {}", out.total);
    }
}

#[test]
fn natural_twenties_and_ones_are_only_a_thing_on_a_d20() {
    let mut r = Rng::from_seed(3);
    let mut saw20 = false;
    let mut saw1 = false;
    for _ in 0..2_000 {
        let out = roll(&mut r, "check", Expr::d20(5), Advantage::Normal);
        saw20 |= out.is_nat20();
        saw1 |= out.is_nat1();
        // A nat 20 is about the die, not the total.
        if out.is_nat20() {
            assert_eq!(out.kept, 20);
            assert_eq!(out.total, 25);
        }
    }
    assert!(saw20 && saw1, "2000 d20 rolls without a 20 or a 1 is suspicious");

    let d6 = roll(&mut r, "damage", Expr::parse("1d6").unwrap(), Advantage::Normal);
    assert!(!d6.is_nat20() && !d6.is_nat1());
}

#[test]
fn the_breakdown_reads_the_way_you_say_it_out_loud() {
    let mut r = Rng::from_seed(2024);
    let out = roll(&mut r, "Stealth", Expr::d20(9), Advantage::Normal);
    let text = out.breakdown();
    assert!(text.starts_with("d20 "), "{text}");
    assert!(text.contains("+9"), "{text}");
    assert!(text.ends_with(&format!("= {}", out.total)), "{text}");

    // Advantage marks which die was kept.
    let adv = roll(&mut r, "Stealth", Expr::d20(9), Advantage::Advantage);
    assert!(adv.breakdown().contains('['), "kept die not marked: {}", adv.breakdown());
}

#[test]
fn entropy_seeding_does_not_produce_the_same_stream_twice() {
    let a: Vec<u32> = (0..10).map(|_| Rng::from_entropy().die(20)).collect();
    let b: Vec<u32> = (0..10).map(|_| Rng::from_entropy().die(20)).collect();
    assert_ne!(a, b, "two entropy-seeded streams were identical");
}

#[test]
fn a_multi_dice_breakdown_keeps_the_count() {
    // "d6 1 5 +3" read like a single-die roll; it must say 2d6.
    let mut r = Rng::from_seed(77);
    let out = roll(&mut r, "damage", Expr::parse("2d6+3").unwrap(), Advantage::Normal);
    assert!(out.breakdown().starts_with("2d6 "), "{}", out.breakdown());

    let single = roll(&mut r, "check", Expr::d20(0), Advantage::Normal);
    assert!(single.breakdown().starts_with("d20 "), "{}", single.breakdown());
}

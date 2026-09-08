//! Visual check: cargo run --example preview_portrait -- <image> [cols] [rows]
fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&a[0])?;
    let cols: usize = a.get(1).map_or(16, |s| s.parse().unwrap());
    let rows: usize = a.get(2).map_or(8, |s| s.parse().unwrap());
    let p = vellum::portrait::Portrait::decode(&bytes, cols, rows)?;
    println!("--- ascii {cols}x{rows} ---");
    for l in p.to_ascii_lines() {
        println!("{l}");
    }
    println!("--- amber half-block (needs truecolour tty) ---");
    for l in p.to_amber_lines(1.6) {
        println!("{l}");
    }
    Ok(())
}

//! What loading the text font costs, in memory and time (T024, NFR-003).
//!
//! Ignored by default: it reads system fonts and measures this process's RSS,
//! so it is a measurement rather than a check. Results are in
//! `docs/evidence/E015-performance-on-the-e001-machine.md`. Run with:
//!
//! ```sh
//! cargo test --release -p ink-render --test font_cost -- --ignored --nocapture --test-threads=1
//! ```

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("VmRSS"))
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
#[ignore]
fn font_memory() {
    let before = rss_kb();
    let font = ink_render::text::TextFont::discover().unwrap();
    let after = rss_kb();
    println!(
        "MEM | ink_render::text::TextFont::discover (main + {} fallbacks): +{} MB",
        font.fallbacks().len(),
        (after - before) / 1024
    );
    for fb in font.fallbacks() {
        let size = std::fs::metadata(fb).unwrap().len() / 1024 / 1024;
        let b = rss_kb();
        let face =
            fontdue::Font::from_bytes(std::fs::read(fb).unwrap(), fontdue::FontSettings::default())
                .unwrap();
        let a = rss_kb();
        println!(
            "MEM | {} ({} MB file, {} glyphs): +{} MB",
            fb.display(),
            size,
            face.glyph_count(),
            (a - b) / 1024
        );
        drop(face);
    }
}

/// The largest fallback on the E001 machine. Absent elsewhere, in which case
/// the timing test says so and stops.
const CJK: &str = "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc";

#[test]
#[ignore]
fn font_load_time() {
    let t = std::time::Instant::now();
    let font = ink_render::text::TextFont::discover().unwrap();
    println!(
        "TIME | discover with {} fallbacks: {:.0} ms",
        font.fallbacks().len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    if !std::path::Path::new(CJK).exists() {
        println!("TIME | {CJK} is not on this machine");
        return;
    }
    let t = std::time::Instant::now();
    let _cjk = fontdue::Font::from_bytes(
        std::fs::read(CJK).unwrap(),
        fontdue::FontSettings::default(),
    )
    .unwrap();
    println!(
        "TIME | CJK face alone: {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
}

//! Release-only budgets on this Mac's real app folders: `cargo test --release --test perf -- --ignored --nocapture`.

use std::time::Instant;

/// p50 / p95 / max (ms) of `f` over `n` runs.
fn times(n: usize, mut f: impl FnMut()) -> (f64, f64, f64) {
    let mut t: Vec<f64> = (0..n)
        .map(|_| {
            let s = Instant::now();
            f();
            s.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (t[n / 2], t[n * 95 / 100], t[n - 1])
}

/// Plan §4.3: a cold scan over 150 ms would need a disk cache of the list.
#[test]
#[ignore]
fn scan_under_150ms() {
    let roots = hop_lib::apps::roots(&[]);
    let first = Instant::now();
    let n = hop_lib::apps::index(&roots).len();
    println!("first scan (cold-ish, includes Launch Services warm-up): {n} apps in {:.1} ms", first.elapsed().as_secs_f64() * 1000.0);
    assert!(n > 0, "no apps found");
    let (p50, p95, max) = times(20, || drop(hop_lib::apps::scan(&roots)));
    println!("walk only: p50 {p50:.2} ms, p95 {p95:.2} ms, max {max:.2} ms");
    let (p50, p95, max) = times(20, || drop(hop_lib::apps::index(&roots)));
    println!("walk + display names: p50 {p50:.2} ms, p95 {p95:.2} ms, max {max:.2} ms");
    assert!(p95 < 150.0, "index p95 {p95:.1} ms");
}

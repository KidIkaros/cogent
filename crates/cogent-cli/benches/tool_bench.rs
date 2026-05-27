use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures")
}

fn bench_tools(c: &mut Criterion) {
    let mut group = c.benchmark_group("individual_tools");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    let fixtures = fixture_dir();
    let small = fixtures.join("small");

    if !small.exists() {
        eprintln!(
            "Fixture directory {} not found, skipping tool benchmarks",
            small.display()
        );
        group.finish();
        return;
    }

    let tools: [(&str, Vec<&str>); 21] = [
        ("secrets", vec![]),
        ("sast", vec![]),
        ("debt", vec![]),
        ("dupfind", vec![]),
        ("deadcode", vec![]),
        ("linelen", vec![]),
        ("comments", vec![]),
        ("coupling", vec![]),
        ("cohesion", vec![]),
        ("halstead", vec![]),
        ("crap", vec![]),
        ("riskmap", vec![]),
        ("cryptocheck", vec![]),
        ("errhandle", vec![]),
        ("taint", vec![]),
        ("typecov", vec![]),
        ("propcov", vec![]),
        ("fuzz", vec![]),
        ("licenses", vec![]),
        ("supply-chain", vec![]),
        ("access-control", vec![]),
    ];

    for (tool, extra_args) in &tools {
        let binary = format!("target/release/{}", tool);
        if !Path::new(&binary).exists() {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::new(*tool, "small_fixture"),
            &small,
            |b, path| {
                b.iter(|| {
                    let mut cmd = Command::new(&binary);
                    cmd.arg(path).arg("--format").arg("json");
                    for arg in extra_args {
                        cmd.arg(arg);
                    }
                    cmd.output().expect("tool should run")
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_tools);
criterion_main!(benches);

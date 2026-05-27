use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures")
}

fn bench_cogent_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("cogent_check");
    group.measurement_time(Duration::from_secs(10));
    group.sample_size(10);

    let fixtures = fixture_dir();

    for size in &["small", "medium", "large"] {
        let path = fixtures.join(size);
        if path.exists() {
            group.bench_with_input(BenchmarkId::new("e2e", *size), &path, |b, path| {
                b.iter(|| {
                    Command::new("cargo")
                        .args(["run", "--release", "-p", "cogent-cli", "--", "check"])
                        .arg(path)
                        .arg("--format")
                        .arg("json")
                        .arg("--force")
                        .output()
                        .expect("cogent check should run")
                });
            });
        }
    }

    group.finish();
}

fn bench_report_formats(c: &mut Criterion) {
    let mut group = c.benchmark_group("report_generation");
    group.measurement_time(Duration::from_secs(10));

    let fixtures = fixture_dir();
    let small = fixtures.join("small");

    if small.exists() {
        for format in &["html", "markdown", "sarif"] {
            group.bench_with_input(BenchmarkId::new(*format, "small"), &format, |b, _fmt| {
                b.iter(|| {
                    Command::new("cargo")
                        .args(["run", "--release", "-p", "cogent-cli", "--", "check"])
                        .arg(&small)
                        .arg("--format")
                        .arg(format.to_string())
                        .arg("--force")
                        .output()
                        .expect("report generation should succeed")
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_cogent_check, bench_report_formats);
criterion_main!(benches);

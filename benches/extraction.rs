use criterion::{Criterion, criterion_group, criterion_main};
use docs_mcp::internal::extractor::extract_content;
use std::fs::{self};
use std::hint::black_box;
use std::path::Path;

pub fn criterion_benchmark(c: &mut Criterion) {
    let test_page_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/testpage_sqlx_any.html");
    let test_page = fs::read_to_string(test_page_path).expect("can read test file");
    c.bench_function("extraction", |b| {
        b.iter(|| extract_content(black_box(&test_page)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

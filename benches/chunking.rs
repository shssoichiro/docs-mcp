use criterion::{Criterion, criterion_group, criterion_main};
use docs_mcp::internal::chunking::ChunkingConfig;
use docs_mcp::internal::chunking::chunk_content;
use docs_mcp::internal::extractor::extract_content;
use std::fs::{self};
use std::hint::black_box;
use std::path::Path;

pub fn criterion_benchmark(c: &mut Criterion) {
    let test_page_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/testpage_sqlx_any.html");
    let test_page = fs::read_to_string(test_page_path).expect("can read test file");
    let content = extract_content(&test_page).unwrap();
    let config = ChunkingConfig::default();
    c.bench_function("chunking", |b| {
        b.iter(|| chunk_content(black_box(&content), black_box(&config)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

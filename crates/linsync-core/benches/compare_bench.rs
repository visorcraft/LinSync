use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use linsync_core::{
    BinaryCompareOptions, TableCompareOptions, TextCompareOptions, TextDocument, compare_binary,
    compare_documents, compare_tables, compare_text,
};

#[cfg(feature = "image-compare")]
use linsync_core::{ImageCompareMode, ImageCompareOptions, compare_images};

fn generate_lines(count: usize, change_every: usize) -> String {
    (0..count)
        .map(|i| {
            if i % change_every == 0 {
                format!("modified line {i} with unique content alpha")
            } else {
                format!("common line {i} shared between both sides")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn generate_identical_lines(count: usize) -> String {
    (0..count)
        .map(|i| format!("common line {i} shared between both sides"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn generate_csv(rows: usize, cols: usize, change_every: usize) -> String {
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| {
                    if r % change_every == 0 && c == 0 {
                        format!("changed_r{r}_c{c}")
                    } else {
                        format!("val_r{r}_c{c}")
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn text_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_compare");

    group.bench_function("small_100_lines", |b| {
        let left = generate_lines(100, 10);
        let right = generate_identical_lines(100);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("medium_1000_lines", |b| {
        let left = generate_lines(1000, 10);
        let right = generate_identical_lines(1000);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("large_8000_lines", |b| {
        let left = generate_lines(8000, 10);
        let right = generate_identical_lines(8000);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("identical_1000_lines", |b| {
        let text = generate_identical_lines(1000);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &text, "right", &text, &opts));
    });

    group.finish();
}

fn compare_documents_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare_documents");

    group.bench_function("small_100_lines", |b| {
        let left_text = generate_lines(100, 10);
        let right_text = generate_identical_lines(100);
        let opts = TextCompareOptions::default();
        b.iter_batched(
            || {
                (
                    TextDocument::from_text("left", &left_text),
                    TextDocument::from_text("right", &right_text),
                )
            },
            |(left_doc, right_doc)| compare_documents(left_doc, right_doc, &opts),
            BatchSize::SmallInput,
        );
    });

    group.bench_function("medium_1000_lines", |b| {
        let left_text = generate_lines(1000, 10);
        let right_text = generate_identical_lines(1000);
        let opts = TextCompareOptions::default();
        b.iter_batched(
            || {
                (
                    TextDocument::from_text("left", &left_text),
                    TextDocument::from_text("right", &right_text),
                )
            },
            |(left_doc, right_doc)| compare_documents(left_doc, right_doc, &opts),
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn hirschberg_vs_lcs(c: &mut Criterion) {
    let mut group = c.benchmark_group("hirschberg_vs_lcs");

    group.bench_function("lcs_full_table_3999_lines", |b| {
        let left = generate_lines(3999, 15);
        let right = generate_identical_lines(3999);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("hirschberg_4001_lines", |b| {
        let left = generate_lines(4001, 15);
        let right = generate_identical_lines(4001);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("hirschberg_6000_lines", |b| {
        let left = generate_lines(6000, 15);
        let right = generate_identical_lines(6000);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("hirschberg_8000_lines", |b| {
        let left = generate_lines(8000, 15);
        let right = generate_identical_lines(8000);
        let opts = TextCompareOptions::default();
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.finish();
}

fn text_compare_options(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_compare_options");

    group.bench_function("ignore_whitespace_1000_lines", |b| {
        let left = generate_lines(1000, 10);
        let right = generate_identical_lines(1000);
        let opts = TextCompareOptions {
            ignore_whitespace: true,
            ..TextCompareOptions::default()
        };
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.bench_function("ignore_case_1000_lines", |b| {
        let left = generate_lines(1000, 10);
        let right = generate_identical_lines(1000);
        let opts = TextCompareOptions {
            ignore_case: true,
            ..TextCompareOptions::default()
        };
        b.iter(|| compare_text("left", &left, "right", &right, &opts));
    });

    group.finish();
}

fn binary_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("binary_compare");

    group.bench_function("equal_4kb", |b| {
        let data = vec![0xFF_u8; 4096];
        let opts = BinaryCompareOptions::default();
        b.iter(|| compare_binary("left", &data, "right", &data, &opts));
    });

    group.bench_function("equal_1mb", |b| {
        let data = vec![0xAB_u8; 1_000_000];
        let opts = BinaryCompareOptions::default();
        b.iter(|| compare_binary("left", &data, "right", &data, &opts));
    });

    group.bench_function("differing_1mb", |b| {
        let left = vec![0xAB_u8; 1_000_000];
        let mut right = vec![0xAB_u8; 1_000_000];
        for byte in right.iter_mut().step_by(17) {
            *byte = 0xCD;
        }
        let opts = BinaryCompareOptions::default();
        b.iter(|| compare_binary("left", &left, "right", &right, &opts));
    });

    group.finish();
}

fn table_compare(c: &mut Criterion) {
    let mut group = c.benchmark_group("table_compare");

    group.bench_function("positional_100x10", |b| {
        let left = generate_csv(100, 10, 10);
        let right = generate_csv(100, 10, usize::MAX);
        let opts = TableCompareOptions::default();
        b.iter(|| compare_tables("left", &left, "right", &right, &opts));
    });

    group.bench_function("positional_1000x10", |b| {
        let left = generate_csv(1000, 10, 10);
        let right = generate_csv(1000, 10, usize::MAX);
        let opts = TableCompareOptions::default();
        b.iter(|| compare_tables("left", &left, "right", &right, &opts));
    });

    group.bench_function("key_column_100x10", |b| {
        let left = generate_csv(100, 10, 10);
        let right = generate_csv(100, 10, usize::MAX);
        let opts = TableCompareOptions {
            key_columns: vec![0],
            ..TableCompareOptions::default()
        };
        b.iter(|| compare_tables("left", &left, "right", &right, &opts));
    });

    group.bench_function("key_column_1000x10", |b| {
        let left = generate_csv(1000, 10, 10);
        let right = generate_csv(1000, 10, usize::MAX);
        let opts = TableCompareOptions {
            key_columns: vec![0],
            ..TableCompareOptions::default()
        };
        b.iter(|| compare_tables("left", &left, "right", &right, &opts));
    });

    group.bench_function("key_column_unordered_1000x10", |b| {
        let left = generate_csv(1000, 10, 10);
        let right = generate_csv(1000, 10, usize::MAX);
        let opts = TableCompareOptions {
            key_columns: vec![0],
            ignore_row_order: true,
            ..TableCompareOptions::default()
        };
        b.iter(|| compare_tables("left", &left, "right", &right, &opts));
    });

    group.finish();
}

#[cfg(feature = "image-compare")]
fn image_compare(c: &mut Criterion) {
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct ImageFixture {
        #[allow(dead_code)]
        dir: TempDir,
        left_path: PathBuf,
        right_path: PathBuf,
        right_alt_path: PathBuf,
    }

    impl ImageFixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let left_path = dir.path().join("left.png");
            let right_path = dir.path().join("right.png");
            let right_alt_path = dir.path().join("right_alt.png");

            let left_img =
                ::image::RgbaImage::from_pixel(256, 256, ::image::Rgba([128, 64, 32, 255]));
            left_img.save(&left_path).unwrap();

            let right_img =
                ::image::RgbaImage::from_pixel(256, 256, ::image::Rgba([128, 64, 32, 255]));
            right_img.save(&right_path).unwrap();

            let mut alt_img =
                ::image::RgbaImage::from_pixel(256, 256, ::image::Rgba([128, 64, 32, 255]));
            for y in 0..256 {
                for x in 0..256 {
                    if (x + y) % 11 == 0 {
                        alt_img.put_pixel(x, y, ::image::Rgba([200, 100, 50, 255]));
                    }
                }
            }
            alt_img.save(&right_alt_path).unwrap();

            Self {
                dir,
                left_path,
                right_path,
                right_alt_path,
            }
        }
    }

    let mut group = c.benchmark_group("image_compare");

    let fix = ImageFixture::new();

    group.bench_function("exact_identical", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Exact,
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_path, &opts).unwrap());
    });

    group.bench_function("exact_differing", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Exact,
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_alt_path, &opts).unwrap());
    });

    group.bench_function("tolerance_identical", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance(10),
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_path, &opts).unwrap());
    });

    group.bench_function("tolerance_differing", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Tolerance(10),
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_alt_path, &opts).unwrap());
    });

    group.bench_function("perceptual_identical", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 2.3,
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_path, &opts).unwrap());
    });

    group.bench_function("perceptual_differing", |b| {
        let opts = ImageCompareOptions {
            mode: ImageCompareMode::Perceptual,
            delta_e_threshold: 2.3,
            ..ImageCompareOptions::default()
        };
        b.iter(|| compare_images(&fix.left_path, &fix.right_alt_path, &opts).unwrap());
    });

    group.finish();
}

#[cfg(feature = "image-compare")]
criterion_group!(
    benches,
    text_compare,
    compare_documents_bench,
    hirschberg_vs_lcs,
    text_compare_options,
    binary_compare,
    table_compare,
    image_compare,
);

#[cfg(not(feature = "image-compare"))]
criterion_group!(
    benches,
    text_compare,
    compare_documents_bench,
    hirschberg_vs_lcs,
    text_compare_options,
    binary_compare,
    table_compare,
);

criterion_main!(benches);

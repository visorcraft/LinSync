use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

use linsync_core::{
    BinaryCompareOptions, CompareMethod, FolderCompareOptions, TableCompareOptions,
    TextCompareOptions, TextDocument, compare_binary, compare_documents, compare_folders,
    compare_tables, compare_text,
};
use linsync_core::{
    CURRENT_PLUGIN_SCHEMA_VERSION, PluginClass, PluginError, PluginExecutionOptions,
    PluginManifest, PluginSandbox, discover_plugins, run_plugin_helper,
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

struct FolderTreeFixture {
    _dir: tempfile::TempDir,
    left: PathBuf,
    right: PathBuf,
}

impl FolderTreeFixture {
    fn new(dir_count: usize, files_per_dir: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();

        for dir_index in 0..dir_count {
            let relative_dir = PathBuf::from(format!("group_{:02}", dir_index % 32))
                .join(format!("leaf_{dir_index:04}"));
            let left_dir = left.join(&relative_dir);
            let right_dir = right.join(&relative_dir);
            fs::create_dir_all(&left_dir).unwrap();
            fs::create_dir_all(&right_dir).unwrap();

            for file_index in 0..files_per_dir {
                let filename = format!("file_{file_index:02}.txt");
                let content = format!("folder benchmark {dir_index}:{file_index}\n");
                let right_content = if file_index == 0 && dir_index % 17 == 0 {
                    format!("folder benchmark changed {dir_index}:{file_index}\n")
                } else {
                    content.clone()
                };

                fs::write(left_dir.join(&filename), &content).unwrap();
                fs::write(right_dir.join(&filename), right_content).unwrap();
            }

            if dir_index % 53 == 0 {
                fs::write(left_dir.join("left_only.txt"), "left only\n").unwrap();
            }

            if dir_index % 59 == 0 {
                fs::write(right_dir.join("right_only.txt"), "right only\n").unwrap();
            }
        }

        Self {
            _dir: dir,
            left,
            right,
        }
    }
}

fn folder_tree_compare(c: &mut Criterion) {
    let fixture = FolderTreeFixture::new(512, 8);
    let mut group = c.benchmark_group("folder_tree_compare");
    group.sample_size(10);

    group.bench_function("huge_tree_existence_4096_files", |b| {
        let opts = FolderCompareOptions {
            compare_method: CompareMethod::Existence,
            ..FolderCompareOptions::default()
        };
        b.iter(|| compare_folders(&fixture.left, &fixture.right, &opts).unwrap());
    });

    group.bench_function("huge_tree_binary_contents_4096_files", |b| {
        let opts = FolderCompareOptions {
            compare_method: CompareMethod::BinaryContents,
            ..FolderCompareOptions::default()
        };
        b.iter(|| compare_folders(&fixture.left, &fixture.right, &opts).unwrap());
    });

    group.finish();
}

struct PluginBenchFixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    fast_dir: PathBuf,
    slow_dir: PathBuf,
    fast_manifest: PluginManifest,
    slow_manifest: PluginManifest,
}

impl PluginBenchFixture {
    fn new(discovery_plugins: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("plugins");
        fs::create_dir_all(&root).unwrap();

        for i in 0..discovery_plugins {
            let plugin_dir = root.join(format!("discover-{i:03}"));
            fs::create_dir_all(&plugin_dir).unwrap();
            write_plugin_helper(&plugin_dir, "probe.sh", FAST_PLUGIN_SCRIPT);
            write_plugin_manifest(
                &plugin_dir,
                &plugin_manifest(&format!("example.discover-{i:03}"), "probe.sh"),
            );
        }

        let fast_dir = root.join("fast");
        fs::create_dir_all(&fast_dir).unwrap();
        write_plugin_helper(&fast_dir, "fast.sh", FAST_PLUGIN_SCRIPT);
        let fast_manifest = plugin_manifest("example.fast-startup", "fast.sh");
        write_plugin_manifest(&fast_dir, &fast_manifest);

        let slow_dir = root.join("slow");
        fs::create_dir_all(&slow_dir).unwrap();
        write_plugin_helper(&slow_dir, "slow.sh", SLOW_PLUGIN_SCRIPT);
        let slow_manifest = plugin_manifest("example.timeout", "slow.sh");
        write_plugin_manifest(&slow_dir, &slow_manifest);

        Self {
            _dir: dir,
            root,
            fast_dir,
            slow_dir,
            fast_manifest,
            slow_manifest,
        }
    }
}

const FAST_PLUGIN_SCRIPT: &str = r#"#!/bin/sh
read request || true
printf '{"ok":true}\n'
"#;

const SLOW_PLUGIN_SCRIPT: &str = r#"#!/bin/sh
echo "started" >&2
sleep 1
"#;

fn plugin_manifest(id: &str, entry: &str) -> PluginManifest {
    PluginManifest {
        schema_version: CURRENT_PLUGIN_SCHEMA_VERSION,
        id: id.to_owned(),
        name: "Benchmark Plugin".to_owned(),
        version: "1.0.0".to_owned(),
        license: "MIT".to_owned(),
        entry: vec![entry.to_owned()],
        classes: vec![PluginClass::Prediffer],
        mime_types: vec!["text/plain".to_owned()],
        extensions: vec!["txt".to_owned()],
        capabilities: vec!["benchmark".to_owned()],
        deterministic: true,
        sandbox: PluginSandbox::default(),
        streaming: false,
        options_schema: vec![],
    }
}

fn write_plugin_manifest(plugin_dir: &Path, manifest: &PluginManifest) {
    let text = serde_json::to_string_pretty(manifest).unwrap();
    fs::write(
        plugin_dir.join(linsync_core::plugin::PLUGIN_MANIFEST_FILE),
        text,
    )
    .unwrap();
}

fn write_plugin_helper(plugin_dir: &Path, name: &str, script: &str) {
    let path = plugin_dir.join(name);
    fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
    }
}

fn plugin_startup_timeout(c: &mut Criterion) {
    let fixture = PluginBenchFixture::new(18);
    let roots = vec![fixture.root.clone()];
    let startup_options = PluginExecutionOptions {
        timeout: Duration::from_secs(2),
        ..PluginExecutionOptions::default()
    };
    let timeout_options = PluginExecutionOptions {
        timeout: Duration::from_millis(10),
        ..PluginExecutionOptions::default()
    };

    let mut group = c.benchmark_group("plugin_startup_timeout");
    group.sample_size(10);

    group.bench_function("discover_20_plugins", |b| {
        b.iter(|| {
            let discovery = discover_plugins(&roots);
            assert_eq!(discovery.plugins.len(), 20);
            assert!(discovery.errors.is_empty());
        });
    });

    group.bench_function("helper_startup_fast_response", |b| {
        b.iter(|| {
            let result = run_plugin_helper(
                &fixture.fast_dir,
                &fixture.fast_manifest,
                "{\"operation\":\"probe\"}\n",
                &startup_options,
            )
            .unwrap();
            assert!(result.stdout.contains("\"ok\""));
        });
    });

    group.bench_function("helper_timeout_10ms", |b| {
        b.iter(|| {
            let err = run_plugin_helper(
                &fixture.slow_dir,
                &fixture.slow_manifest,
                "{}",
                &timeout_options,
            )
            .unwrap_err();
            assert!(matches!(err, PluginError::TimedOut { .. }));
        });
    });

    group.finish();
}

#[cfg(feature = "image-compare")]
fn image_compare(c: &mut Criterion) {
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
    folder_tree_compare,
    plugin_startup_timeout,
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
    folder_tree_compare,
    plugin_startup_timeout,
);

criterion_main!(benches);

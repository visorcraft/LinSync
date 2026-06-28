use super::*;

pub(crate) fn hex_command(args: &[String]) -> Result<ExitCode, String> {
    let mut bytes_per_row = 16;
    let mut metadata_only = false;
    let mut output = OutputMode::Text;
    let mut paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                set_output_mode(&mut output, OutputMode::Json, "--json")?;
                index += 1;
            }
            "--count" => {
                set_output_mode(&mut output, OutputMode::Count, "--count")?;
                index += 1;
            }
            "--quiet" | "-q" => {
                set_output_mode(&mut output, OutputMode::Quiet, "--quiet")?;
                index += 1;
            }
            "--width" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--width requires a byte count".to_owned());
                };
                bytes_per_row = value
                    .parse::<usize>()
                    .map_err(|_| "--width must be a positive integer".to_owned())?
                    .max(1);
                index += 2;
            }
            "--metadata-only" => {
                metadata_only = true;
                index += 1;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli hex [--width BYTES] [--metadata-only] LEFT RIGHT".to_owned(),
        );
    }

    let result = compare_binary_files(
        PathBuf::from(&paths[0]).as_path(),
        PathBuf::from(&paths[1]).as_path(),
        &BinaryCompareOptions {
            bytes_per_row,
            compare_content: !metadata_only,
            compare_metadata: metadata_only,
        },
    )
    .map_err(|err| err.to_string())?;
    let difference_count = result.differences.len() + result.metadata_differences.len();

    match output {
        OutputMode::Text => {
            println!(
                "left_len={} right_len={} differing_bytes={} metadata_differences={} content_compared={}",
                result.left_len,
                result.right_len,
                result.differences.len(),
                result.metadata_differences.len(),
                result.content_compared
            );
            if !result.metadata_differences.is_empty() {
                println!(
                    "metadata: {}",
                    result
                        .metadata_differences
                        .iter()
                        .map(|difference| difference.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            for row in result.rows.iter().filter(|row| row.has_difference).take(12) {
                println!(
                    "{:08X} | {:<48} | {:<48} | {} | {}",
                    row.offset, row.left_hex, row.right_hex, row.left_ascii, row.right_ascii
                );
            }
        }
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "equal": result.is_equal(),
                    "left_len": result.left_len,
                    "right_len": result.right_len,
                    "differences": difference_count,
                    "differing_bytes": result.differences.len(),
                    "metadata_differences": result.metadata_differences.iter().map(|difference| difference.as_str()).collect::<Vec<_>>(),
                    "content_compared": result.content_compared,
                    "metadata": result.metadata.as_ref().map(|metadata| {
                        serde_json::json!({
                            "left": {
                                "len": metadata.left.len,
                                "modified_ms": metadata.left.modified.and_then(system_time_millis),
                                "readonly": metadata.left.readonly,
                            },
                            "right": {
                                "len": metadata.right.len,
                                "modified_ms": metadata.right.modified.and_then(system_time_millis),
                                "readonly": metadata.right.readonly,
                            },
                        })
                    }),
                })
            );
        }
        OutputMode::Count => println!("{difference_count}"),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

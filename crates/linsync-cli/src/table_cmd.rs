use super::*;

pub(crate) fn table_command(args: &[String]) -> Result<ExitCode, String> {
    let mut delimiter = ',';
    let mut has_header = false;
    let mut output = OutputMode::Text;
    let mut paths = Vec::new();
    let mut index = 0;
    let mut table_opts = TableCompareOptions::default();

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
            "--delimiter" | "-d" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--delimiter requires a character".to_owned());
                };
                delimiter = parse_delimiter(value)?;
                index += 2;
            }
            "--tsv" => {
                delimiter = '\t';
                index += 1;
            }
            "--header" => {
                has_header = true;
                index += 1;
            }
            "--table-quote" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-quote requires a character".to_owned());
                };
                table_opts.quote_char = Some(parse_single_char(value, "--table-quote")?);
                index += 2;
            }
            "--table-escape" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-escape requires a character".to_owned());
                };
                table_opts.escape_char = Some(parse_single_char(value, "--table-escape")?);
                index += 2;
            }
            "--table-comment" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-comment requires a prefix string".to_owned());
                };
                table_opts.comment_prefix = Some(value.clone());
                index += 2;
            }
            "--table-skip-blank" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--table-skip-blank requires true/false".to_owned());
                };
                table_opts.skip_blank_rows = parse_cli_bool(value, "--table-skip-blank")?;
                index += 2;
            }
            "--numeric-tolerance" => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--numeric-tolerance requires a number".to_owned());
                };
                let tolerance = value
                    .parse::<f64>()
                    .map_err(|_| format!("--numeric-tolerance requires a number, got '{value}'"))?;
                if !tolerance.is_finite() || tolerance < 0.0 {
                    return Err(
                        "--numeric-tolerance requires a finite non-negative number".to_owned()
                    );
                }
                table_opts.numeric_tolerance = Some(tolerance);
                index += 2;
            }
            value => {
                paths.push(value.to_owned());
                index += 1;
            }
        }
    }

    if paths.len() != 2 {
        return Err(
            "usage: linsync-cli table [--header] [--delimiter CHAR|--tsv] [--table-quote CHAR] [--table-escape CHAR] [--table-comment PREFIX] [--table-skip-blank BOOL] [--numeric-tolerance FLOAT] LEFT RIGHT".to_owned(),
        );
    }

    table_opts.delimiter = delimiter;
    table_opts.has_header = has_header;

    let result = compare_table_files(
        PathBuf::from(&paths[0]).as_path(),
        PathBuf::from(&paths[1]).as_path(),
        &table_opts,
    )
    .map_err(|err| err.to_string())?;

    match output {
        OutputMode::Text => {
            println!(
                "rows={} changed_cells={}",
                result.rows.len(),
                result.changed_cells
            );
            for row in result.rows.iter().filter(|row| row.has_difference).take(20) {
                for cell in row
                    .cells
                    .iter()
                    .filter(|cell| cell.state != TableCellState::Equal)
                {
                    println!(
                        "row={} col={} left={} right={}",
                        row.row_index + 1,
                        cell.column_index + 1,
                        cell.left.as_deref().unwrap_or(""),
                        cell.right.as_deref().unwrap_or("")
                    );
                }
            }
        }
        OutputMode::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "equal": result.is_equal(),
                    "rows": result.rows.len(),
                    "changed_cells": result.changed_cells,
                })
            );
        }
        OutputMode::Count => println!("{}", result.changed_cells),
        OutputMode::Quiet => {}
    }

    Ok(if result.is_equal() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

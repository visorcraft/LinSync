use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TableCompareOptions {
    pub delimiter: char,
    pub has_header: bool,
}

impl Default for TableCompareOptions {
    fn default() -> Self {
        Self {
            delimiter: ',',
            has_header: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCompareResult {
    pub left_name: String,
    pub right_name: String,
    pub header: Option<Vec<String>>,
    pub rows: Vec<TableRowDiff>,
    pub changed_cells: usize,
}

impl TableCompareResult {
    pub fn is_equal(&self) -> bool {
        self.changed_cells == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRowDiff {
    pub row_index: usize,
    pub cells: Vec<TableCellDiff>,
    pub has_difference: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCellDiff {
    pub column_index: usize,
    pub left: Option<String>,
    pub right: Option<String>,
    pub state: TableCellState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableCellState {
    Equal,
    Changed,
    LeftOnly,
    RightOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableParseError {
    pub message: String,
}

impl std::fmt::Display for TableParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TableParseError {}

pub fn compare_table_files(
    left: &Path,
    right: &Path,
    options: &TableCompareOptions,
) -> Result<TableCompareResult, TableError> {
    let left_text = fs::read_to_string(left)?;
    let right_text = fs::read_to_string(right)?;
    Ok(compare_tables(
        &left.display().to_string(),
        &left_text,
        &right.display().to_string(),
        &right_text,
        options,
    )?)
}

#[derive(Debug)]
pub enum TableError {
    Io(io::Error),
    Parse(TableParseError),
}

impl std::fmt::Display for TableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for TableError {}

impl From<io::Error> for TableError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TableParseError> for TableError {
    fn from(value: TableParseError) -> Self {
        Self::Parse(value)
    }
}

pub fn compare_tables(
    left_name: &str,
    left: &str,
    right_name: &str,
    right: &str,
    options: &TableCompareOptions,
) -> Result<TableCompareResult, TableParseError> {
    let mut left_rows = parse_delimited(left, options.delimiter)?;
    let mut right_rows = parse_delimited(right, options.delimiter)?;
    let header = if options.has_header {
        let left_header = (!left_rows.is_empty()).then(|| left_rows.remove(0));
        let right_header = (!right_rows.is_empty()).then(|| right_rows.remove(0));
        left_header.or(right_header)
    } else {
        None
    };

    let max_rows = left_rows.len().max(right_rows.len());
    let mut rows = Vec::new();
    let mut changed_cells = 0;

    for row_index in 0..max_rows {
        let left_row = left_rows.get(row_index);
        let right_row = right_rows.get(row_index);
        let max_cols = left_row
            .map_or(0, Vec::len)
            .max(right_row.map_or(0, Vec::len));
        let mut cells = Vec::new();
        let mut has_difference = false;

        for column_index in 0..max_cols {
            let left_cell = left_row.and_then(|row| row.get(column_index)).cloned();
            let right_cell = right_row.and_then(|row| row.get(column_index)).cloned();
            let state = match (&left_cell, &right_cell) {
                (Some(left), Some(right)) if left == right => TableCellState::Equal,
                (Some(_), Some(_)) => TableCellState::Changed,
                (Some(_), None) => TableCellState::LeftOnly,
                (None, Some(_)) => TableCellState::RightOnly,
                (None, None) => continue,
            };

            if state != TableCellState::Equal {
                changed_cells += 1;
                has_difference = true;
            }

            cells.push(TableCellDiff {
                column_index,
                left: left_cell,
                right: right_cell,
                state,
            });
        }

        rows.push(TableRowDiff {
            row_index,
            cells,
            has_difference,
        });
    }

    Ok(TableCompareResult {
        left_name: left_name.to_owned(),
        right_name: right_name.to_owned(),
        header,
        rows,
        changed_cells,
    })
}

pub fn parse_delimited(input: &str, delimiter: char) -> Result<Vec<Vec<String>>, TableParseError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = input.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => in_quotes = false,
                _ => cell.push(ch),
            }
            continue;
        }

        if ch == '"' {
            in_quotes = true;
        } else if ch == delimiter {
            row.push(std::mem::take(&mut cell));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(ch);
        }
    }

    if in_quotes {
        return Err(TableParseError {
            message: "unterminated quoted field".to_owned(),
        });
    }

    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_changed_cells() {
        let result = compare_tables(
            "left",
            "name,count\nalpha,1\nbeta,2\n",
            "right",
            "name,count\nalpha,1\nbeta,3\n",
            &TableCompareOptions {
                delimiter: ',',
                has_header: true,
            },
        )
        .unwrap();

        assert!(!result.is_equal());
        assert_eq!(
            result.header.as_ref().unwrap(),
            &vec!["name".to_owned(), "count".to_owned()]
        );
        assert_eq!(result.changed_cells, 1);
        assert_eq!(result.rows[1].cells[1].state, TableCellState::Changed);
    }

    #[test]
    fn supports_quoted_multiline_fields() {
        let rows = parse_delimited("name,body\nalpha,\"one\ntwo\"\n", ',').unwrap();

        assert_eq!(rows[1][1], "one\ntwo");
    }

    #[test]
    fn supports_tsv() {
        let result = compare_tables(
            "left",
            "a\tb\n1\t2\n",
            "right",
            "a\tb\n1\t3\n",
            &TableCompareOptions {
                delimiter: '\t',
                has_header: false,
            },
        )
        .unwrap();

        assert_eq!(result.changed_cells, 1);
    }
}

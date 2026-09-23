//! A small RFC 4180 reader, written by hand to keep the core free of a CSV
//! dependency.
//!
//! Quoted fields may hold the delimiter, newlines and doubled quotes. Lines
//! end with LF, CRLF or a lone CR. A UTF-8 BOM is dropped, every field is
//! trimmed, and records whose fields are all empty (a blank line, or the
//! `;;;;` some spreadsheets leave at the end) are skipped. Nothing here
//! fails: a quote left open is reported on the record it swallowed.

use std::iter::Peekable;
use std::str::Chars;

/// The delimiters [`sniff_delimiter`] chooses among, in order of preference.
const CANDIDATES: [char; 3] = [',', ';', '\t'];

/// Records after the header that [`sniff_delimiter`] looks at.
const SNIFF_ROWS: usize = 20;

/// One record of the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Record {
    /// 1-based line the record starts on. A record with a newline inside
    /// quotes spans several lines; the next record's number accounts for them.
    pub line: u32,
    pub fields: Vec<String>,
    /// A quoted field was still open at the end of the file, so this record
    /// swallowed everything after it.
    pub unclosed_quote: bool,
}

/// The records of `text`, split on `delimiter`.
pub(super) fn records(text: &str, delimiter: char) -> Records<'_> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    Records {
        chars: text.chars().peekable(),
        delimiter,
        line: 1,
    }
}

/// Iterator returned by [`records`].
pub(super) struct Records<'a> {
    chars: Peekable<Chars<'a>>,
    delimiter: char,
    /// Line of the next character.
    line: u32,
}

impl Records<'_> {
    /// Consumes the `\n` of a CRLF after a `\r` and counts the line.
    fn end_of_line(&mut self, c: char) {
        if c == '\r' && self.chars.peek() == Some(&'\n') {
            self.chars.next();
        }
        self.line = self.line.saturating_add(1);
    }

    /// One record, blank ones included; `None` at the end of the text.
    fn next_raw(&mut self) -> Option<Record> {
        self.chars.peek()?;
        let line = self.line;
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut quoted = false;
        loop {
            let Some(c) = self.chars.next() else {
                fields.push(finish(field));
                return Some(Record {
                    line,
                    fields,
                    unclosed_quote: quoted,
                });
            };
            if quoted {
                match c {
                    '"' if self.chars.peek() == Some(&'"') => {
                        self.chars.next();
                        field.push('"');
                    }
                    '"' => quoted = false,
                    '\r' | '\n' => {
                        self.end_of_line(c);
                        field.push('\n');
                    }
                    _ => field.push(c),
                }
                continue;
            }
            match c {
                // An opening quote, possibly after some spaces. A quote in the
                // middle of an unquoted field is kept as it is.
                '"' if field.trim().is_empty() => {
                    field.clear();
                    quoted = true;
                }
                '\r' | '\n' => {
                    self.end_of_line(c);
                    fields.push(finish(field));
                    return Some(Record {
                        line,
                        fields,
                        unclosed_quote: false,
                    });
                }
                c if c == self.delimiter => fields.push(finish(std::mem::take(&mut field))),
                _ => field.push(c),
            }
        }
    }
}

impl Iterator for Records<'_> {
    type Item = Record;

    fn next(&mut self) -> Option<Record> {
        loop {
            let record = self.next_raw()?;
            if record.unclosed_quote || record.fields.iter().any(|f| !f.is_empty()) {
                return Some(record);
            }
        }
    }
}

fn finish(field: String) -> String {
    let trimmed = field.trim();
    if trimmed.len() == field.len() {
        field
    } else {
        trimmed.to_string()
    }
}

/// The delimiter among `,`, `;` and tab that splits the header into the most
/// columns the following rows agree with. Rows with the header's column count
/// weigh first, the column count second, the order of [`CANDIDATES`] breaks
/// ties. A file no candidate splits falls back to `,`.
pub(super) fn sniff_delimiter(text: &str) -> char {
    let mut best = (CANDIDATES[0], 0usize, 0usize);
    for delimiter in CANDIDATES {
        let mut rows = records(text, delimiter).take(SNIFF_ROWS + 1);
        let Some(header) = rows.next() else {
            return CANDIDATES[0];
        };
        let columns = header.fields.len();
        if columns < 2 {
            continue;
        }
        let agreeing = rows.filter(|r| r.fields.len() == columns).count();
        if (agreeing, columns) > (best.1, best.2) {
            best = (delimiter, agreeing, columns);
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(text: &str, delimiter: char) -> Vec<Vec<String>> {
        records(text, delimiter).map(|r| r.fields).collect()
    }

    fn row(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn quoted_fields_hold_delimiters_quotes_and_newlines() {
        let text = "a,b,c\n\"x, y\",\"say \"\"hi\"\"\",\"two\nlines\"\nlast,,row\n";
        assert_eq!(
            fields(text, ','),
            vec![
                row(&["a", "b", "c"]),
                row(&["x, y", "say \"hi\"", "two\nlines"]),
                row(&["last", "", "row"]),
            ]
        );
    }

    #[test]
    fn lines_count_the_newlines_inside_quotes() {
        let text = "h1,h2\n\"one\r\ntwo\",x\r\nthree,y\r\n";
        let lines: Vec<u32> = records(text, ',').map(|r| r.line).collect();
        assert_eq!(lines, vec![1, 2, 4]);
    }

    #[test]
    fn bom_crlf_blank_lines_and_padding_are_dropped() {
        let text = "\u{feff}name ; amount\r\n\r\n  Bar  ;\t3,50 \r\n;\r\n\r\n";
        assert_eq!(
            fields(text, ';'),
            vec![row(&["name", "amount"]), row(&["Bar", "3,50"])]
        );
    }

    #[test]
    fn a_lone_cr_ends_a_line_and_the_last_line_needs_no_newline() {
        assert_eq!(
            fields("a,b\rc,d", ','),
            vec![row(&["a", "b"]), row(&["c", "d"])]
        );
    }

    #[test]
    fn spaces_before_an_opening_quote_are_ignored() {
        assert_eq!(fields("a, \"b,c\" ,d", ','), vec![row(&["a", "b,c", "d"])]);
    }

    #[test]
    fn a_quote_inside_an_unquoted_field_is_literal() {
        assert_eq!(fields("5\" screen,x", ','), vec![row(&["5\" screen", "x"])]);
    }

    #[test]
    fn an_unclosed_quote_swallows_the_rest_and_says_so() {
        let all: Vec<Record> = records("a,b\n\"open,x\ny,z\n", ',').collect();
        assert_eq!(all.len(), 2);
        assert!(!all[0].unclosed_quote);
        assert!(all[1].unclosed_quote);
        assert_eq!(all[1].fields, row(&["open,x\ny,z"]));
    }

    #[test]
    fn the_delimiter_is_the_one_the_rows_agree_with() {
        assert_eq!(sniff_delimiter("a,b,c\n1,2,3\n4,5,6\n"), ',');
        assert_eq!(
            sniff_delimiter("Data;Descrizione;Importo\n01/09/2026;Bar, caffè;-1,50\n"),
            ';'
        );
        assert_eq!(sniff_delimiter("a\tb\n1\t2\n"), '\t');
        assert_eq!(sniff_delimiter("single\nvalue\n"), ',');
        assert_eq!(sniff_delimiter(""), ',');
    }
}

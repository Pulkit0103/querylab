//! Parsing of imported files: CSV and JSON (an array of objects or one
//! object per line), column naming, type inference and the COPY text
//! encoding. Pure functions over bytes, independent of the database.

use std::ops::ControlFlow;

use serde::de::{DeserializeSeed, IgnoredAny, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::{
    errors::{AppError, AppResult},
    models::data_jobs::{
        FileFormat, ImportPreview, InferredType, ParseOptions, RowError, SourceColumn,
        MAX_REPORTED_ERRORS, PREVIEW_ROWS, SAMPLE_ROWS,
    },
};

/// How a file is read, resolved from the options and the file itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// `Csv` or `Json`, never `Auto`.
    pub format: FileFormat,
    pub delimiter: u8,
    pub has_header: bool,
    /// JSON: an array of objects rather than one object per line.
    pub json_array: bool,
    pub columns: Vec<String>,
}

/// One data row. `row` is 1-based and counts every data row, readable or
/// not; `line` is where the row starts in the file (CSV and NDJSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub row: u64,
    pub line: Option<u64>,
    pub values: Vec<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Record(Record),
    Error(RowError),
}

fn strip_bom(data: &[u8]) -> &[u8] {
    data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data)
}

fn first_significant(data: &[u8]) -> Option<u8> {
    data.iter().copied().find(|b| !b.is_ascii_whitespace())
}

/// The delimiter that occurs most often on the first line (outside quotes).
pub fn detect_delimiter(data: &[u8]) -> u8 {
    let mut counts = [(b',', 0usize), (b';', 0), (b'\t', 0), (b'|', 0)];
    let mut quoted = false;
    for &b in data.iter().take(64 * 1024) {
        match b {
            b'"' => quoted = !quoted,
            b'\n' if !quoted => break,
            _ if !quoted => {
                if let Some(c) = counts.iter_mut().find(|(d, _)| *d == b) {
                    c.1 += 1;
                }
            }
            _ => {}
        }
    }
    counts
        .iter()
        .max_by_key(|(d, n)| (*n, *d == b','))
        .map(|(d, _)| *d)
        .unwrap_or(b',')
}

/// Trimmed, non-empty and unique column names.
pub fn column_names<I: IntoIterator<Item = String>>(raw: I) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (i, name) in raw.into_iter().enumerate() {
        let base: String = name.trim().chars().take(63).collect();
        let base = if base.is_empty() {
            format!("column_{}", i + 1)
        } else {
            base
        };
        let mut candidate = base.clone();
        let mut n = 2;
        while names.iter().any(|existing| existing == &candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        names.push(candidate);
    }
    names
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn csv_reader(data: &[u8], delimiter: u8) -> csv::Reader<&[u8]> {
    csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(data)
}

fn csv_error(err: csv::Error) -> AppError {
    AppError::BadRequest(format!("the CSV file cannot be read: {err}"))
}

fn json_error(err: serde_json::Error) -> AppError {
    AppError::BadRequest(format!("the JSON file cannot be read: {err}"))
}

/// A JSON object, or why a record is unusable.
type JsonRecord = Result<Map<String, Value>, String>;

fn as_object(value: Value) -> JsonRecord {
    match value {
        Value::Object(map) => Ok(map),
        other => Err(format!(
            "expected an object, found {}",
            match other {
                Value::Array(_) => "an array",
                Value::String(_) => "a string",
                Value::Number(_) => "a number",
                Value::Bool(_) => "a boolean",
                _ => "null",
            }
        )),
    }
}

/// Calls `f(row, line, record)` for each element of a top-level array,
/// without materialising the whole document.
struct ArraySeed<'f>(&'f mut dyn FnMut(u64, JsonRecord) -> ControlFlow<()>);

impl<'de> DeserializeSeed<'de> for ArraySeed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, de: D) -> Result<(), D::Error> {
        de.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ArraySeed<'_> {
    type Value = ();
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an array of objects")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut row = 0u64;
        let mut stopped = false;
        while let Some(value) = seq.next_element::<Value>()? {
            row += 1;
            if (self.0)(row, as_object(value)).is_break() {
                stopped = true;
                break;
            }
        }
        if stopped {
            while seq.next_element::<IgnoredAny>()?.is_some() {}
        }
        Ok(())
    }
}

fn for_each_json(
    data: &[u8],
    array: bool,
    f: &mut dyn FnMut(u64, Option<u64>, JsonRecord) -> ControlFlow<()>,
) -> AppResult<()> {
    if array {
        let mut de = serde_json::Deserializer::from_slice(data);
        let mut each = |row: u64, rec: JsonRecord| f(row, None, rec);
        ArraySeed(&mut each)
            .deserialize(&mut de)
            .map_err(json_error)?;
        de.end().map_err(json_error)?;
        return Ok(());
    }
    let mut row = 0u64;
    for (i, line) in data.split(|&b| b == b'\n').enumerate() {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        row += 1;
        let record = serde_json::from_slice::<Value>(line)
            .map_err(|e| format!("invalid JSON: {e}"))
            .and_then(as_object);
        if f(row, Some(i as u64 + 1), record).is_break() {
            break;
        }
    }
    Ok(())
}

/// Resolves the format, delimiter and column names.
pub fn resolve(data: &[u8], opts: &ParseOptions) -> AppResult<Layout> {
    let data = strip_bom(data);
    let first = first_significant(data);
    let format = match opts.format {
        FileFormat::Auto if matches!(first, Some(b'[' | b'{')) => FileFormat::Json,
        FileFormat::Auto => FileFormat::Csv,
        f => f,
    };
    match format {
        FileFormat::Json => {
            let json_array = first == Some(b'[');
            // Keys of the sampled objects, in order of first appearance.
            let mut columns: Vec<String> = Vec::new();
            let mut sampled = 0usize;
            for_each_json(data, json_array, &mut |_, _, record| {
                if let Ok(map) = record {
                    for key in map.keys() {
                        if !columns.iter().any(|c| c == key) {
                            columns.push(key.clone());
                        }
                    }
                }
                sampled += 1;
                if sampled >= SAMPLE_ROWS {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })?;
            if columns.is_empty() {
                return Err(AppError::BadRequest(
                    "the JSON file contains no objects with fields".into(),
                ));
            }
            Ok(Layout {
                format,
                delimiter: b',',
                has_header: false,
                json_array,
                columns,
            })
        }
        _ => {
            let delimiter = match opts.delimiter {
                Some(d) => d as u8,
                None => detect_delimiter(data),
            };
            let mut reader = csv_reader(data, delimiter);
            let mut first = csv::ByteRecord::new();
            if !reader.read_byte_record(&mut first).map_err(csv_error)? {
                return Err(AppError::BadRequest("the file is empty".into()));
            }
            let columns = if opts.has_header {
                column_names(
                    first
                        .iter()
                        .map(|f| String::from_utf8_lossy(f).into_owned()),
                )
            } else {
                (1..=first.len()).map(|i| format!("column_{i}")).collect()
            };
            Ok(Layout {
                format: FileFormat::Csv,
                delimiter,
                has_header: opts.has_header,
                json_array: false,
                columns,
            })
        }
    }
}

/// Visits every data row. A file that cannot be read past some point (a
/// JSON syntax error inside an array) is an error; unreadable rows are
/// reported as [`Item::Error`] and reading continues.
pub fn for_each(
    data: &[u8],
    layout: &Layout,
    mut f: impl FnMut(Item) -> ControlFlow<()>,
) -> AppResult<()> {
    let data = strip_bom(data);
    match layout.format {
        FileFormat::Json => for_each_json(data, layout.json_array, &mut |row, line, record| {
            f(match record {
                Ok(map) => Item::Record(Record {
                    row,
                    line,
                    values: layout
                        .columns
                        .iter()
                        .map(|c| map.get(c).and_then(json_text))
                        .collect(),
                }),
                Err(message) => Item::Error(RowError {
                    row,
                    line,
                    column: None,
                    message,
                }),
            })
        }),
        _ => {
            let mut reader = csv_reader(data, layout.delimiter);
            let mut record = csv::ByteRecord::new();
            let expected = layout.columns.len();
            let mut row = 0u64;
            let mut header_pending = layout.has_header;
            while reader.read_byte_record(&mut record).map_err(csv_error)? {
                if header_pending {
                    header_pending = false;
                    continue;
                }
                row += 1;
                let line = record.position().map(|p| p.line());
                let item = if record.len() != expected {
                    Item::Error(RowError {
                        row,
                        line,
                        column: None,
                        message: format!("expected {expected} fields, found {}", record.len()),
                    })
                } else {
                    let values: Result<Vec<Option<String>>, usize> = record
                        .iter()
                        .enumerate()
                        .map(|(i, field)| {
                            std::str::from_utf8(field)
                                .map(|s| Some(s.to_string()))
                                .map_err(|_| i)
                        })
                        .collect();
                    match values {
                        Ok(values) => Item::Record(Record { row, line, values }),
                        Err(i) => Item::Error(RowError {
                            row,
                            line,
                            column: Some(layout.columns[i].clone()),
                            message: "the value is not valid UTF-8".into(),
                        }),
                    }
                };
                if f(item).is_break() {
                    break;
                }
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Type inference
// ---------------------------------------------------------------------------

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

fn is_integer(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    digits(body) && body.len() <= 18
}

fn is_decimal(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    let (mantissa, exponent) = match body.split_once(['e', 'E']) {
        Some((m, e)) => (m, Some(e)),
        None => (body, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((a, b)) => {
            (digits(a) || a.is_empty())
                && (digits(b) || b.is_empty())
                && !(a.is_empty() && b.is_empty())
        }
        None => digits(mantissa),
    };
    mantissa_ok && absent_or_digits(exponent.map(|e| e.strip_prefix(['-', '+']).unwrap_or(e)))
}

fn absent_or_digits(s: Option<&str>) -> bool {
    s.map_or(true, digits)
}

fn is_boolean(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "t" | "f" | "yes" | "no" | "y" | "n"
    )
}

fn number_in(s: &str, len: usize, min: u32, max: u32) -> bool {
    s.len() == len && digits(s) && s.parse::<u32>().is_ok_and(|n| (min..=max).contains(&n))
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && s.is_ascii()
        && b[4] == b'-'
        && b[7] == b'-'
        && number_in(&s[0..4], 4, 1, 9999)
        && number_in(&s[5..7], 2, 1, 12)
        && number_in(&s[8..10], 2, 1, 31)
}

fn is_time(s: &str) -> bool {
    // HH:MM[:SS[.fraction]]
    if !s.is_ascii() {
        return false;
    }
    let (hm, rest) = s.split_at(s.len().min(5));
    let b = hm.as_bytes();
    if !(b.len() == 5
        && b[2] == b':'
        && number_in(&hm[0..2], 2, 0, 23)
        && number_in(&hm[3..5], 2, 0, 59))
    {
        return false;
    }
    if rest.is_empty() {
        return true;
    }
    let Some(sec) = rest.strip_prefix(':') else {
        return false;
    };
    let (whole, fraction) = match sec.split_once('.') {
        Some((w, f)) => (w, Some(f)),
        None => (sec, None),
    };
    number_in(whole, 2, 0, 60) && absent_or_digits(fraction)
}

fn is_timestamp(s: &str) -> bool {
    if s.len() < 16
        || !s.is_ascii()
        || !is_date(&s[..10])
        || !matches!(s.as_bytes()[10], b'T' | b' ')
    {
        return false;
    }
    let rest = &s[11..];
    // Zone: Z, or +HH[:MM] / -HH[:MM] after the time.
    let (time, zone) = if let Some(t) = rest.strip_suffix(['Z', 'z']) {
        (t, None)
    } else if let Some(pos) = rest.rfind(['+', '-']).filter(|&p| p >= 5) {
        (&rest[..pos], Some(&rest[pos + 1..]))
    } else {
        (rest, None)
    };
    let zone_ok = match zone {
        None => true,
        Some(z) => {
            let z = z.replace(':', "");
            matches!(z.len(), 2 | 4) && digits(&z)
        }
    };
    zone_ok && is_time(time.trim_end())
}

fn is_uuid(s: &str) -> bool {
    s.len() == 36 && uuid::Uuid::parse_str(s).is_ok()
}

fn is_json_document(s: &str) -> bool {
    let t = s.trim_start();
    (t.starts_with('{') || t.starts_with('[')) && serde_json::from_str::<IgnoredAny>(s).is_ok()
}

/// Statistics for one column over sampled values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inference {
    pub inferred: InferredType,
    pub empty: u64,
    pub max_length: usize,
}

/// The most specific type every non-empty value satisfies.
pub fn infer<'a>(values: impl IntoIterator<Item = Option<&'a str>>) -> Inference {
    let (mut int, mut dec, mut boolean, mut date, mut ts, mut uuid, mut json) =
        (true, true, true, true, true, true, true);
    let mut seen = 0u64;
    let mut empty = 0u64;
    let mut max_length = 0usize;
    for v in values {
        let Some(v) = v.filter(|v| !v.is_empty()) else {
            empty += 1;
            continue;
        };
        seen += 1;
        max_length = max_length.max(v.chars().count());
        let t = v.trim();
        int &= is_integer(t);
        dec &= int || is_decimal(t);
        boolean &= is_boolean(t);
        date &= is_date(t);
        ts &= date || is_timestamp(t);
        uuid &= is_uuid(t);
        json &= is_json_document(v);
    }
    let inferred = if seen == 0 {
        InferredType::Unknown
    } else if int {
        InferredType::Integer
    } else if dec {
        InferredType::Decimal
    } else if boolean {
        InferredType::Boolean
    } else if date {
        InferredType::Date
    } else if ts {
        InferredType::Timestamp
    } else if uuid {
        InferredType::Uuid
    } else if json {
        InferredType::Json
    } else {
        InferredType::Text
    };
    Inference {
        inferred,
        empty,
        max_length,
    }
}

/// Reads the whole file once: counts rows, collects unreadable ones and
/// infers column types from the first [`SAMPLE_ROWS`] rows.
pub fn analyze(data: &[u8], opts: &ParseOptions) -> AppResult<(Layout, ImportPreview)> {
    let layout = resolve(data, opts)?;
    let mut sample: Vec<Vec<Option<String>>> = Vec::new();
    let mut total = 0u64;
    let mut parse_errors = Vec::new();
    let mut parse_error_count = 0u64;
    for_each(data, &layout, |item| {
        total += 1;
        match item {
            Item::Record(r) if sample.len() < SAMPLE_ROWS => sample.push(r.values),
            Item::Record(_) => {}
            Item::Error(e) => {
                parse_error_count += 1;
                if parse_errors.len() < MAX_REPORTED_ERRORS {
                    parse_errors.push(e);
                }
            }
        }
        ControlFlow::Continue(())
    })?;
    let columns = layout
        .columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let inf = infer(sample.iter().map(|r| r[i].as_deref()));
            SourceColumn {
                index: i,
                name: name.clone(),
                inferred_type: inf.inferred,
                empty_count: inf.empty,
                max_length: inf.max_length,
            }
        })
        .collect();
    let preview = ImportPreview {
        import_id: uuid::Uuid::nil(),
        format: layout.format,
        delimiter: (layout.format == FileFormat::Csv).then_some(layout.delimiter as char),
        has_header: layout.has_header,
        columns,
        sampled_rows: sample.len() as u64,
        rows: sample.into_iter().take(PREVIEW_ROWS).collect(),
        total_rows: total,
        parse_errors,
        parse_error_count,
    };
    Ok((layout, preview))
}

// ---------------------------------------------------------------------------
// COPY text format
// ---------------------------------------------------------------------------

/// Appends a value in PostgreSQL's COPY text format (`\N` for NULL).
pub fn copy_value(out: &mut Vec<u8>, value: Option<&str>) {
    let Some(v) = value else {
        out.extend_from_slice(b"\\N");
        return;
    };
    for b in v.bytes() {
        match b {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> ParseOptions {
        ParseOptions::default()
    }

    fn collect(data: &[u8], layout: &Layout) -> Vec<Item> {
        let mut items = Vec::new();
        for_each(data, layout, |i| {
            items.push(i);
            ControlFlow::Continue(())
        })
        .unwrap();
        items
    }

    fn record(item: &Item) -> &Record {
        match item {
            Item::Record(r) => r,
            Item::Error(e) => panic!("unexpected error {e:?}"),
        }
    }

    #[test]
    fn csv_with_header_quotes_and_multiline_values() {
        let data =
            "\u{feff}id,name, note \n1,Ada,\"says \"\"hi\"\"\"\n2,Grace,\"two\nlines\"\n3,Linus\n"
                .as_bytes();
        let layout = resolve(data, &opts()).unwrap();
        assert_eq!(layout.format, FileFormat::Csv);
        assert_eq!(layout.columns, ["id", "name", "note"]);
        let items = collect(data, &layout);
        assert_eq!(items.len(), 3);
        assert_eq!(
            record(&items[0]).values,
            [
                Some("1".into()),
                Some("Ada".into()),
                Some("says \"hi\"".into())
            ]
        );
        assert_eq!(record(&items[1]).values[2].as_deref(), Some("two\nlines"));
        assert_eq!(record(&items[1]).line, Some(3));
        match &items[2] {
            Item::Error(e) => {
                assert_eq!((e.row, e.line), (3, Some(5)));
                assert_eq!(e.message, "expected 3 fields, found 2");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn delimiters_are_detected_and_headers_optional() {
        assert_eq!(detect_delimiter(b"a;b;c\n1;2;3"), b';');
        assert_eq!(detect_delimiter(b"a\tb\n"), b'\t');
        assert_eq!(detect_delimiter(b"\"x;y\",b\n"), b',');
        assert_eq!(detect_delimiter(b"single\n"), b',');
        let o = ParseOptions {
            has_header: false,
            ..opts()
        };
        let layout = resolve(b"1|2\n3|4\n", &o).unwrap();
        assert_eq!(layout.delimiter, b'|');
        assert_eq!(layout.columns, ["column_1", "column_2"]);
        assert_eq!(collect(b"1|2\n3|4\n", &layout).len(), 2);
    }

    #[test]
    fn column_names_are_unique_and_never_empty() {
        let names = column_names(["a", " a ", "", "a_2", "a"].map(String::from));
        assert_eq!(names, ["a", "a_2", "column_3", "a_2_2", "a_3"]);
    }

    #[test]
    fn json_arrays_and_ndjson() {
        let array = br#" [{"id": 1, "name": "Ada", "tags": ["x"]}, {"id": 2, "extra": true}, 5]"#;
        let layout = resolve(array, &opts()).unwrap();
        assert!(layout.json_array);
        assert_eq!(layout.columns, ["id", "name", "tags", "extra"]);
        let items = collect(array, &layout);
        assert_eq!(
            record(&items[0]).values,
            [
                Some("1".into()),
                Some("Ada".into()),
                Some("[\"x\"]".into()),
                None
            ]
        );
        assert_eq!(record(&items[1]).values[3].as_deref(), Some("true"));
        match &items[2] {
            Item::Error(e) => assert_eq!(
                (e.row, e.line, e.message.as_str()),
                (3, None, "expected an object, found a number")
            ),
            other => panic!("{other:?}"),
        }

        let nd = b"{\"a\": \"x\"}\r\n\n{\"a\": null}\n{oops}\n";
        let layout = resolve(nd, &opts()).unwrap();
        assert!(!layout.json_array);
        let items = collect(nd, &layout);
        assert_eq!(record(&items[0]).values, [Some("x".into())]);
        assert_eq!(record(&items[1]).line, Some(3));
        assert_eq!(record(&items[1]).values, [None]);
        match &items[2] {
            Item::Error(e) => assert_eq!((e.row, e.line), (3, Some(4))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn broken_json_arrays_are_rejected() {
        let data = br#"[{"a": 1}, {"a": "#;
        let layout = Layout {
            format: FileFormat::Json,
            delimiter: b',',
            has_header: false,
            json_array: true,
            columns: vec!["a".into()],
        };
        assert!(for_each(data, &layout, |_| ControlFlow::Continue(())).is_err());
        assert!(resolve(b"[]", &opts()).is_err());
        assert!(resolve(b"", &opts()).is_err());
    }

    #[test]
    fn type_inference() {
        let t = |vals: &[&str]| infer(vals.iter().map(|v| Some(*v))).inferred;
        assert_eq!(t(&["1", "-20", ""]), InferredType::Integer);
        assert_eq!(t(&["1", "2.5", "-3e4", ".5"]), InferredType::Decimal);
        assert_eq!(t(&["true", "No", "f"]), InferredType::Boolean);
        assert_eq!(t(&["2024-02-29", "1999-12-31"]), InferredType::Date);
        assert_eq!(
            t(&[
                "2024-02-29",
                "2024-03-01T10:20:30Z",
                "2024-03-01 10:20:30.123+05:30",
                "2024-03-01 10:20"
            ]),
            InferredType::Timestamp
        );
        assert_eq!(t(&["2024-13-01"]), InferredType::Text);
        assert_eq!(
            t(&["7f1d6b0e-8d4a-4b8e-9c55-0d2a0b5f6a11"]),
            InferredType::Uuid
        );
        assert_eq!(t(&["{\"a\": 1}", "[1, 2]"]), InferredType::Json);
        assert_eq!(t(&["1", "x"]), InferredType::Text);
        assert_eq!(t(&["", ""]), InferredType::Unknown);
        let inf = infer([Some("abc"), None, Some(""), Some("ab")]);
        assert_eq!((inf.empty, inf.max_length), (2, 3));
    }

    #[test]
    fn analysis_counts_rows_and_errors() {
        let mut data = String::from("n,v\n");
        for i in 0..1500 {
            data.push_str(&format!("{i},{}\n", i * 2));
        }
        data.push_str("bad\n");
        let (layout, preview) = analyze(data.as_bytes(), &opts()).unwrap();
        assert_eq!(layout.columns, ["n", "v"]);
        assert_eq!(preview.total_rows, 1501);
        assert_eq!(preview.sampled_rows, SAMPLE_ROWS as u64);
        assert_eq!(preview.rows.len(), PREVIEW_ROWS);
        assert_eq!(preview.parse_error_count, 1);
        assert_eq!(preview.parse_errors[0].line, Some(1502));
        assert_eq!(preview.columns[0].inferred_type, InferredType::Integer);
        assert_eq!(preview.delimiter, Some(','));
    }

    #[test]
    fn copy_text_escaping() {
        let mut out = Vec::new();
        copy_value(&mut out, Some("a\tb\\c\nd\re"));
        out.push(b'\t');
        copy_value(&mut out, None);
        assert_eq!(out, b"a\\tb\\\\c\\nd\\re\t\\N");
    }
}

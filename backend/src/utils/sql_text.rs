//! Lightweight SQL text helpers. These are for display and UX only and are
//! **not** a security boundary — enforcement is done by PostgreSQL itself
//! (read-only transactions, role privileges, single-statement protocol).

/// Returns the leading keyword of a statement (uppercased), skipping
/// whitespace, comments and opening parentheses. `None` for empty input.
pub fn leading_keyword(sql: &str) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b if b.is_ascii_whitespace() || b == b'(' => i += 1,
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                // Block comments nest in PostgreSQL.
                let mut depth = 0;
                while i < bytes.len() {
                    if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            _ => break,
        }
    }
    let word: String = sql[i..]
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '_')
        .collect();
    (!word.is_empty()).then(|| word.to_ascii_uppercase())
}

/// Whether the statement reports affected rows rather than a result set.
pub fn is_dml(keyword: Option<&str>) -> bool {
    matches!(keyword, Some("INSERT" | "UPDATE" | "DELETE" | "MERGE"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_leading_keyword() {
        assert_eq!(leading_keyword("select 1").as_deref(), Some("SELECT"));
        assert_eq!(
            leading_keyword("  \n-- hi\n  with x as (select 1) select * from x").as_deref(),
            Some("WITH")
        );
        assert_eq!(
            leading_keyword("/* a /* nested */ b */ update t set x = 1").as_deref(),
            Some("UPDATE")
        );
        assert_eq!(leading_keyword("((select 1))").as_deref(), Some("SELECT"));
        assert_eq!(leading_keyword("-- only a comment"), None);
        assert_eq!(leading_keyword(""), None);
        assert_eq!(leading_keyword("/* unterminated"), None);
    }

    #[test]
    fn dml_detection() {
        assert!(is_dml(Some("DELETE")));
        assert!(!is_dml(Some("SELECT")));
        assert!(!is_dml(None));
    }
}

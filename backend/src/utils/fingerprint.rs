//! Query fingerprints: equivalent statements (differing only in literals,
//! parameters, whitespace, comments, keyword case or the length of value
//! lists) normalise to the same text and hash.
//!
//! This is a small tokenizer, not a parser. It is used to group history for
//! performance timelines and is never a security boundary.

/// Keywords after which a `-` starts a negative number rather than a
/// subtraction.
const UNARY_CONTEXT: &[&str] = &[
    "select", "where", "and", "or", "not", "then", "else", "when", "values", "in", "by", "limit",
    "offset", "return", "set", "between", "like", "ilike", "is", "case", "having", "on", "any",
    "all", "array",
];

/// Keywords followed by a space before `(` in normalised text.
const SPACED_KEYWORDS: &[&str] = &[
    "in",
    "values",
    "from",
    "join",
    "as",
    "on",
    "using",
    "where",
    "and",
    "or",
    "not",
    "exists",
    "select",
    "into",
    "set",
    "when",
    "then",
    "else",
    "over",
    "with",
    "by",
    "union",
    "all",
    "except",
    "intersect",
    "returning",
    "lateral",
    "any",
    "some",
    "is",
    "between",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// Literal or parameter placeholder.
    Value,
    /// Lower-cased unquoted identifier or keyword.
    Word(String),
    /// Quoted identifier, kept verbatim (quotes included).
    Quoted(String),
    /// Operator or punctuation.
    Symbol(String),
}

impl Token {
    fn is(&self, s: &str) -> bool {
        matches!(self, Token::Symbol(t) if t == s)
    }

    fn text(&self) -> &str {
        match self {
            Token::Value => "?",
            Token::Word(s) | Token::Quoted(s) | Token::Symbol(s) => s,
        }
    }
}

const OPERATOR_CHARS: &str = "+-*/<>=~!@#%^&|`?";

fn tokenize(sql: &str) -> Vec<Token> {
    let chars: Vec<char> = sql.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0;
    let at = |i: usize| chars.get(i).copied();

    while i < chars.len() {
        let c = chars[i];
        // Whitespace and comments.
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && at(i + 1) == Some('-') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && at(i + 1) == Some('*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at(i + 1) == Some('/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // String literal ('' escapes; E'' strings also allow backslash escapes).
        if c == '\'' {
            let backslash = matches!(out.last(), Some(Token::Word(w)) if w == "e");
            if matches!(out.last(), Some(Token::Word(w)) if matches!(w.as_str(), "e" | "b" | "x" | "n"))
            {
                out.pop();
            }
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '\\' if backslash => i += 2,
                    '\'' if at(i + 1) == Some('\'') => i += 2,
                    '\'' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            out.push(Token::Value);
            continue;
        }
        // Positional parameter or dollar-quoted string.
        if c == '$' {
            if at(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                i += 1;
                while at(i).is_some_and(|d| d.is_ascii_digit()) {
                    i += 1;
                }
                out.push(Token::Value);
                continue;
            }
            let mut j = i + 1;
            while at(j).is_some_and(|d| d.is_alphanumeric() || d == '_') {
                j += 1;
            }
            if at(j) == Some('$') {
                let tag: Vec<char> = chars[i..=j].to_vec();
                let mut k = j + 1;
                let end = loop {
                    if k + tag.len() > chars.len() {
                        break chars.len();
                    }
                    if chars[k..k + tag.len()] == tag[..] {
                        break k + tag.len();
                    }
                    k += 1;
                };
                i = end;
                out.push(Token::Value);
                continue;
            }
        }
        // Quoted identifier ("" escapes).
        if c == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if at(i + 1) == Some('"') {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(Token::Quoted(chars[start..i].iter().collect()));
            continue;
        }
        // Numbers (including .5, 1e-3 and 0x1F).
        if c.is_ascii_digit() || (c == '.' && at(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            i += 1;
            while let Some(d) = at(i) {
                let exponent_sign = (d == '+' || d == '-') && matches!(at(i - 1), Some('e' | 'E'));
                if d.is_ascii_alphanumeric() || d == '.' || d == '_' || exponent_sign {
                    i += 1;
                } else {
                    break;
                }
            }
            // A preceding unary minus belongs to the number.
            if out.last().is_some_and(|t| t.is("-")) {
                let before = out.len().checked_sub(2).map(|p| &out[p]);
                let unary = match before {
                    None => true,
                    Some(Token::Symbol(s)) => s != ")" && s != "]",
                    Some(Token::Word(w)) => UNARY_CONTEXT.contains(&w.as_str()),
                    _ => false,
                };
                if unary {
                    out.pop();
                }
            }
            out.push(Token::Value);
            continue;
        }
        // Identifiers and keywords.
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while at(i).is_some_and(|d| d.is_alphanumeric() || d == '_' || d == '$') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
            out.push(Token::Word(word));
            continue;
        }
        // Casts, operators and punctuation.
        if c == ':' && at(i + 1) == Some(':') {
            out.push(Token::Symbol("::".into()));
            i += 2;
            continue;
        }
        if OPERATOR_CHARS.contains(c) {
            let start = i;
            while at(i).is_some_and(|d| OPERATOR_CHARS.contains(d)) {
                i += 1;
            }
            out.push(Token::Symbol(chars[start..i].iter().collect()));
            continue;
        }
        out.push(Token::Symbol(c.to_string()));
        i += 1;
    }
    out
}

/// `( ?, ?, ? )` and `[ ?, ? ]` become `( ? )` / `[ ? ]`, then repeated
/// `( ? ), ( ? )` rows (multi-row VALUES) become one.
fn collapse_lists(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    for token in tokens {
        let closes = token.is(")") || token.is("]");
        out.push(token);
        if !closes {
            continue;
        }
        let open = if out.last().is_some_and(|t| t.is(")")) {
            "("
        } else {
            "["
        };
        // Walk back over `? , ? , ?`.
        let mut j = out.len() - 1;
        let mut values = 0;
        loop {
            if j == 0 {
                break;
            }
            j -= 1;
            match &out[j] {
                Token::Value => values += 1,
                t if t.is(",") => {}
                t if t.is(open) => break,
                _ => {
                    values = 0;
                    break;
                }
            }
        }
        if values > 0 && out[j].is(open) && j + 1 < out.len() {
            let close = out.pop().unwrap();
            out.truncate(j + 1);
            out.push(Token::Value);
            out.push(close);
            // `(?) , (?)` -> `(?)`
            let n = out.len();
            if n >= 7
                && out[n - 3].is("(")
                && out[n - 4].is(",")
                && out[n - 5].is(")")
                && out[n - 6] == Token::Value
                && out[n - 7].is("(")
            {
                out.truncate(n - 4);
            }
        }
    }
    out
}

/// Normalised statement text: literals and parameters become `?`, unquoted
/// words are lower-cased, comments and extra whitespace are removed, value
/// lists are collapsed and trailing semicolons dropped.
pub fn normalize(sql: &str) -> String {
    let mut tokens = collapse_lists(tokenize(sql));
    while tokens.last().is_some_and(|t| t.is(";")) {
        tokens.pop();
    }
    let mut out = String::with_capacity(sql.len().min(4096));
    let mut prev: Option<&Token> = None;
    for t in &tokens {
        let text = t.text();
        // `f(x)` and `a[1]`, but `in (...)` and `values (...)`.
        let call = (text == "(" || text == "[")
            && match prev {
                Some(Token::Word(w)) => !SPACED_KEYWORDS.contains(&w.as_str()),
                Some(Token::Quoted(_)) => true,
                Some(p) => text == "[" && (p.is(")") || p.is("]")),
                None => false,
            };
        let tight_before = call || matches!(text, "," | ")" | "]" | "." | "::" | ";");
        let tight_after = prev.is_some_and(|p| matches!(p.text(), "(" | "[" | "." | "::"));
        if prev.is_some() && !tight_before && !tight_after {
            out.push(' ');
        }
        out.push_str(text);
        prev = Some(t);
    }
    out
}

/// The first word (lower case) of every statement in a script. String
/// literals and dollar-quoted bodies are skipped, so `BEGIN` inside a
/// function body is not reported.
pub fn statement_heads(sql: &str) -> Vec<String> {
    let mut heads = Vec::new();
    let mut at_start = true;
    for token in tokenize(sql) {
        if token.is(";") {
            at_start = true;
            continue;
        }
        if at_start {
            if let Token::Word(w) = &token {
                heads.push(w.clone());
            }
            at_start = token.is("(");
        }
    }
    heads
}

/// 64-bit FNV-1a hash of the normalised text (stable across releases, so
/// stored fingerprints stay comparable).
pub fn fingerprint(sql: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalize(sql).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_literals_case_and_whitespace() {
        assert_eq!(
            normalize("SELECT *\n  FROM Orders WHERE id = 42 AND status = 'paid';"),
            "select * from orders where id = ? and status = ?"
        );
        assert_eq!(
            normalize(
                "select a.b::text, f(x) from t -- note\n/* block /* nested */ */ where y > 1.5e-3"
            ),
            "select a.b::text, f(x) from t where y > ?"
        );
    }

    #[test]
    fn equivalent_statements_share_a_fingerprint() {
        let same = [
            "SELECT * FROM orders WHERE id = 1",
            "select *  from ORDERS where id=2;",
            "select * from orders where id = $1",
            "select * from orders /* hi */ where id = -7",
            "select * from orders where id = 'abc'",
        ];
        let fp = fingerprint(same[0]);
        for sql in same {
            assert_eq!(fingerprint(sql), fp, "{sql} -> {}", normalize(sql));
        }
        assert_ne!(fingerprint("select * from customers where id = 1"), fp);
        assert_ne!(fingerprint("select * from orders where id > 1"), fp);
    }

    #[test]
    fn collapses_value_lists_and_rows() {
        assert_eq!(
            normalize("select 1 from t where id in (1, 2, 3)"),
            normalize("select 1 from t where id IN (4)")
        );
        assert_eq!(
            normalize("insert into t (a, b) values (1, 'x'), (2, 'y'), (3, 'z')"),
            "insert into t(a, b) values (?)"
        );
        assert_eq!(normalize("select array[1, 2, 3]"), "select array[?]");
        // Lists containing expressions are kept.
        assert_eq!(normalize("select f(a, 1)"), "select f(a, ?)");
    }

    #[test]
    fn keeps_identifier_case_when_quoted_and_minus_as_operator() {
        assert_ne!(
            fingerprint(r#"select * from "Orders""#),
            fingerprint(r#"select * from "orders""#)
        );
        assert_eq!(normalize("select a - 1 from t"), "select a - ? from t");
        assert_eq!(normalize("select (a) - 1"), "select (a) - ?");
        assert_eq!(normalize("select -1"), "select ?");
    }

    #[test]
    fn finds_statement_heads() {
        assert_eq!(
            statement_heads("BEGIN; create table t(); -- x\n(select 1); $$commit$$; Commit"),
            ["begin", "create", "select", "commit"]
        );
    }

    #[test]
    fn handles_special_strings() {
        assert_eq!(
            normalize("select E'it\\'s -- not a comment', $tag$ ' $tag$, $$x$$ from t"),
            "select ?, ?, ? from t"
        );
        assert_eq!(normalize("select 'a''b' || x'1F'"), "select ? || ?");
        // Unterminated input does not panic.
        assert_eq!(normalize("select 'open"), "select ?");
        assert_eq!(normalize("select $q$ open"), "select ?");
        assert_eq!(normalize(""), "");
    }
}

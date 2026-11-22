//! Value generators for mock data: a seeded random source, realistic
//! values (names, emails, products, addresses, dates, ...), the default
//! generator for a column, and the bounds and value lists of simple CHECK
//! constraints. Pure functions; same seed, same values.

use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};

use crate::{models::data_jobs::Generator, services::sql_builder::Column};

/// SplitMix64: small, fast and good enough for test data. Not for secrets.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// A generator for one table, independent of the other tables.
    pub fn for_table(seed: u64, schema: &str, table: &str) -> Self {
        // FNV-1a over the qualified name, mixed with the seed.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in schema.bytes().chain(*b".").chain(table.bytes()) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Self(seed ^ h)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }

    /// Uniform in `min..=max`.
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        let span = (max as i128 - min as i128 + 1) as u128;
        if span > u128::from(u64::MAX) {
            return self.next_u64() as i64;
        }
        (min as i128 + self.below(span as u64) as i128) as i64
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const FIRST_NAMES: &[&str] = &[
    "Ada", "Alan", "Amara", "Anika", "Arjun", "Beatriz", "Carlos", "Chen", "Chloe", "Dmitri",
    "Elena", "Emeka", "Fatima", "Grace", "Hana", "Hiro", "Ibrahim", "Isla", "Jamal", "Javier",
    "Johan", "Keisha", "Kenji", "Lars", "Leila", "Liam", "Lucia", "Mateo", "Maya", "Mei", "Nadia",
    "Noah", "Olivia", "Omar", "Priya", "Rafael", "Sofia", "Tariq", "Yara", "Zoe",
];
const LAST_NAMES: &[&str] = &[
    "Adeyemi",
    "Andersen",
    "Bauer",
    "Brown",
    "Chen",
    "Costa",
    "Dubois",
    "Fernandez",
    "Garcia",
    "Haddad",
    "Hughes",
    "Ivanova",
    "Jensen",
    "Kim",
    "Kowalski",
    "Kumar",
    "Larsen",
    "Lopez",
    "Martin",
    "Meyer",
    "Moreau",
    "Nakamura",
    "Nguyen",
    "Novak",
    "Okafor",
    "Olsen",
    "Patel",
    "Petrov",
    "Rossi",
    "Santos",
    "Schmidt",
    "Silva",
    "Singh",
    "Smith",
    "Suzuki",
    "Tanaka",
    "Torres",
    "Wang",
    "Williams",
    "Yilmaz",
];
const WORDS: &[&str] = &[
    "amber", "anchor", "atlas", "beacon", "birch", "bright", "canyon", "cedar", "cobalt", "comet",
    "coral", "crisp", "delta", "ember", "falcon", "fern", "glacier", "harbor", "horizon", "indigo",
    "island", "jade", "lantern", "lumen", "maple", "meadow", "mesa", "nimbus", "north", "orbit",
    "pebble", "pine", "prairie", "quartz", "river", "sage", "summit", "tide", "willow", "zenith",
];
const COMPANY_SUFFIXES: &[&str] = &[
    "Labs",
    "Systems",
    "Group",
    "Partners",
    "Works",
    "Co",
    "Studio",
    "Analytics",
];
const ADJECTIVES: &[&str] = &[
    "Compact",
    "Classic",
    "Durable",
    "Ergonomic",
    "Lightweight",
    "Modern",
    "Portable",
    "Premium",
    "Rustic",
    "Sleek",
    "Smart",
    "Vintage",
];
const MATERIALS: &[&str] = &[
    "Bamboo", "Ceramic", "Cotton", "Glass", "Leather", "Linen", "Oak", "Steel", "Wool", "Granite",
];
const PRODUCTS: &[&str] = &[
    "Backpack",
    "Bottle",
    "Chair",
    "Desk",
    "Headphones",
    "Jacket",
    "Keyboard",
    "Lamp",
    "Mug",
    "Notebook",
    "Speaker",
    "Watch",
];
const CITIES: &[&str] = &[
    "Amsterdam",
    "Austin",
    "Bangalore",
    "Barcelona",
    "Berlin",
    "Buenos Aires",
    "Cape Town",
    "Chicago",
    "Dublin",
    "Lagos",
    "Lisbon",
    "London",
    "Melbourne",
    "Mexico City",
    "Montreal",
    "Nairobi",
    "Osaka",
    "Paris",
    "Seoul",
    "Singapore",
    "Stockholm",
    "Toronto",
];
const COUNTRIES: &[(&str, &str)] = &[
    ("Argentina", "AR"),
    ("Australia", "AU"),
    ("Brazil", "BR"),
    ("Canada", "CA"),
    ("France", "FR"),
    ("Germany", "DE"),
    ("India", "IN"),
    ("Ireland", "IE"),
    ("Japan", "JP"),
    ("Kenya", "KE"),
    ("Mexico", "MX"),
    ("Netherlands", "NL"),
    ("Nigeria", "NG"),
    ("Portugal", "PT"),
    ("South Africa", "ZA"),
    ("South Korea", "KR"),
    ("Spain", "ES"),
    ("Sweden", "SE"),
    ("United Kingdom", "GB"),
    ("United States", "US"),
];
const STREETS: &[&str] = &[
    "Street",
    "Avenue",
    "Road",
    "Lane",
    "Boulevard",
    "Way",
    "Place",
];
const DOMAINS: &[&str] = &["example.com", "example.org", "example.net"];

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn sentence(rng: &mut Rng) -> String {
    let n = rng.range(6, 12) as usize;
    let words: Vec<&str> = (0..n).map(|_| *rng.pick(WORDS)).collect();
    format!("{}.", capitalize(&words.join(" ")))
}

/// Accepts `YYYY-MM-DD` or a date-time.
fn parse_instant(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(t.naive_utc());
    }
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .ok()
}

/// Random instant in `[from, to]`, falling back to the last year.
fn instant(rng: &mut Rng, from: &str, to: &str) -> NaiveDateTime {
    let now = Utc::now().naive_utc();
    let from = parse_instant(from).unwrap_or(now - Duration::days(365));
    let to = parse_instant(to).unwrap_or(now).max(from);
    let span = (to - from).num_seconds().max(0);
    from + Duration::seconds(rng.range(0, span))
}

/// Per-row context for generators that depend on the row.
pub struct RowContext {
    /// 0-based row number within the run for this table.
    pub index: u64,
    /// Distinguishes this run's unique values from earlier runs.
    pub run_tag: u64,
}

/// Produces one value; `None` is SQL NULL. `Default` and `Reference` are
/// handled by the caller and produce `None` here.
pub fn generate(g: &Generator, rng: &mut Rng, ctx: &RowContext) -> Option<String> {
    Some(match g {
        Generator::Default | Generator::Null | Generator::Reference => return None,
        Generator::Constant { value } => value.clone(),
        Generator::FirstName => rng.pick(FIRST_NAMES).to_string(),
        Generator::LastName => rng.pick(LAST_NAMES).to_string(),
        Generator::FullName => format!("{} {}", rng.pick(FIRST_NAMES), rng.pick(LAST_NAMES)),
        Generator::Email => format!(
            "{}.{}{}@{}",
            rng.pick(FIRST_NAMES).to_lowercase(),
            rng.pick(LAST_NAMES).to_lowercase(),
            ctx.run_tag * 1_000_000 + ctx.index,
            rng.pick(DOMAINS)
        ),
        Generator::Username => format!(
            "{}{}{}",
            rng.pick(FIRST_NAMES).to_lowercase(),
            rng.pick(LAST_NAMES)[..1].to_lowercase(),
            rng.range(1, 9999)
        ),
        Generator::Phone => format!("+1-555-{:04}", rng.range(100, 9999)),
        Generator::Company => format!(
            "{} {} {}",
            capitalize(rng.pick(WORDS)),
            capitalize(rng.pick(WORDS)),
            rng.pick(COMPANY_SUFFIXES)
        ),
        Generator::ProductName => format!(
            "{} {} {}",
            rng.pick(ADJECTIVES),
            rng.pick(MATERIALS),
            rng.pick(PRODUCTS)
        ),
        Generator::Word => rng.pick(WORDS).to_string(),
        Generator::Sentence => sentence(rng),
        Generator::Paragraph => {
            let n = rng.range(3, 5);
            (0..n).map(|_| sentence(rng)).collect::<Vec<_>>().join(" ")
        }
        Generator::City => rng.pick(CITIES).to_string(),
        Generator::Country => rng.pick(COUNTRIES).0.to_string(),
        Generator::CountryCode => rng.pick(COUNTRIES).1.to_string(),
        Generator::StreetAddress => format!(
            "{} {} {}",
            rng.range(1, 9999),
            capitalize(rng.pick(WORDS)),
            rng.pick(STREETS)
        ),
        Generator::PostalCode => format!("{:05}", rng.range(1000, 99999)),
        Generator::Url => format!(
            "https://www.{}{}.example/{}",
            rng.pick(WORDS),
            rng.pick(WORDS),
            rng.pick(WORDS)
        ),
        Generator::Uuid => {
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
            bytes[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
            uuid::Builder::from_random_bytes(bytes)
                .into_uuid()
                .to_string()
        }
        Generator::Boolean { true_ratio } => (rng.unit() < *true_ratio).to_string(),
        Generator::Integer { min, max } => rng.range(*min, *max).to_string(),
        Generator::Sequence { start } => {
            (start.unwrap_or(1).saturating_add(ctx.index as i64)).to_string()
        }
        Generator::Decimal { min, max, scale } => {
            let v = min + rng.unit() * (max - min);
            let factor = 10f64.powi(*scale as i32);
            let rounded = ((v * factor).round() / factor).clamp(*min, *max);
            format!("{rounded:.*}", *scale as usize)
        }
        Generator::Date { from, to } => instant(rng, from, to).date().to_string(),
        Generator::Timestamp { from, to } => {
            format!("{}+00", instant(rng, from, to).format("%Y-%m-%d %H:%M:%S"))
        }
        Generator::Values { values } => rng.pick(values).clone(),
        Generator::Json => format!(
            "{{\"label\": \"{}\", \"score\": {}}}",
            rng.pick(WORDS),
            rng.range(1, 100)
        ),
    })
}

/// Generators whose values never repeat within a run.
pub fn is_unique(g: &Generator) -> bool {
    matches!(
        g,
        Generator::Uuid | Generator::Sequence { .. } | Generator::Email | Generator::Default
    )
}

/// Makes a value unique within a run (for UNIQUE text columns), keeping
/// within `max_len` characters.
pub fn uniquify(value: String, ctx: &RowContext, max_len: Option<usize>) -> String {
    let suffix = format!("-{}", ctx.run_tag * 1_000_000 + ctx.index);
    match max_len {
        Some(n) if value.chars().count() + suffix.len() > n => {
            let keep = n.saturating_sub(suffix.len());
            let mut v: String = value.chars().take(keep).collect();
            v.push_str(&suffix);
            v.chars()
                .rev()
                .take(n)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        }
        _ => value + &suffix,
    }
}

pub fn truncate(value: String, max_len: Option<usize>) -> String {
    match max_len {
        Some(n) if value.chars().count() > n => value.chars().take(n).collect(),
        _ => value,
    }
}

/// `(precision, scale)` of `numeric(p,s)`; `(n)` of character types.
pub fn type_modifiers(data_type: &str) -> Vec<u32> {
    data_type
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inner, _)| {
            inner
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Bounds and allowed values from the simple CHECK constraints on a column.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CheckRules {
    pub min: Option<(f64, bool)>,
    pub max: Option<(f64, bool)>,
    pub values: Option<Vec<String>>,
}

fn quoted_literals(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\'' {
            continue;
        }
        let mut lit = String::new();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    lit.push('\'');
                } else {
                    break;
                }
            } else {
                lit.push(c);
            }
        }
        out.push(lit);
    }
    out
}

fn strip_casts(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("::") {
        out.push_str(&rest[..pos]);
        rest = rest[pos + 2..].trim_start_matches(|c: char| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '"' | '[' | ']' | '.')
        });
    }
    out.push_str(rest);
    out
}

fn word_boundary_contains(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(word) {
        let i = start + pos;
        let before = i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        let j = i + word.len();
        let after = j >= bytes.len() || !(bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_');
        if before && after {
            return true;
        }
        start = i + word.len();
    }
    false
}

/// Interprets a CHECK definition (`pg_get_constraintdef`) for `column`.
/// Returns `None` when the constraint is not a simple rule on this column
/// alone (the caller warns that it may be violated).
pub fn parse_check(def: &str, column: &str, all_columns: &[&str]) -> Option<CheckRules> {
    let body = def.trim().strip_prefix("CHECK")?.trim();
    let ident = if column
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        column.to_string()
    } else {
        format!("\"{}\"", column.replace('"', "\"\""))
    };
    if !word_boundary_contains(body, ident.trim_matches('"')) {
        return None;
    }
    // Other columns in the expression: not a single-column rule.
    if all_columns
        .iter()
        .any(|c| *c != column && word_boundary_contains(&strip_casts(body).replace(&ident, ""), c))
    {
        return None;
    }
    let mut rules = CheckRules::default();
    if body.contains("= ANY") {
        if body.contains(" OR ") || body.contains(" AND ") {
            return None;
        }
        rules.values = Some(quoted_literals(body));
        return Some(rules);
    }
    if body.contains(" OR ") || body.contains('\'') {
        return None;
    }
    let flat: String = strip_casts(body).replace(['(', ')'], " ");
    for part in flat.split(" AND ") {
        let tokens: Vec<&str> = part.split_whitespace().collect();
        let [lhs, op, rhs] = tokens.as_slice() else {
            return None;
        };
        let (op, value) = if *lhs == ident {
            (*op, rhs.parse::<f64>().ok()?)
        } else if *rhs == ident {
            // `0 < x` → `x > 0`
            let flipped = match *op {
                "<" => ">",
                "<=" => ">=",
                ">" => "<",
                ">=" => "<=",
                other => other,
            };
            (flipped, lhs.parse::<f64>().ok()?)
        } else {
            return None;
        };
        match op {
            ">" => rules.min = Some((value, true)),
            ">=" => rules.min = Some((value, false)),
            "<" => rules.max = Some((value, true)),
            "<=" => rules.max = Some((value, false)),
            "<>" | "!=" => {}
            _ => return None,
        }
    }
    Some(rules)
}

fn has(name: &str, parts: &[&str]) -> bool {
    parts.iter().any(|p| name.contains(p))
}

/// The generator used when none is chosen, with a note on why. `unique`
/// is true for single-column unique keys.
pub fn default_generator(table: &str, c: &Column, unique: bool) -> (Generator, Option<String>) {
    let name = c.name.to_lowercase();
    let mods = type_modifiers(&c.data_type);
    if c.identity.is_some() || (c.has_default && unique && c.kind() == "number") {
        return (Generator::Default, Some("generated by the database".into()));
    }
    if c.generated {
        return (Generator::Default, Some("generated column".into()));
    }
    if !c.enum_values.is_empty() {
        return (
            Generator::Values {
                values: c.enum_values.clone(),
            },
            None,
        );
    }
    let now = Utc::now().date_naive();
    let years_ago = |y: i64| (now - Duration::days(365 * y)).to_string();
    let g = match c.kind() {
        "boolean" => Generator::Boolean { true_ratio: 0.5 },
        "uuid" => Generator::Uuid,
        "json" => Generator::Json,
        "number" if unique => Generator::Sequence { start: None },
        "number" => {
            let integer = matches!(c.base_type.as_str(), "int2" | "int4" | "int8");
            let money = has(
                &name,
                &[
                    "price", "amount", "total", "cost", "salary", "balance", "fee",
                ],
            );
            if integer {
                let max = if has(&name, &["age"]) {
                    90
                } else if has(&name, &["qty", "quantity", "count", "stock"]) {
                    100
                } else {
                    1000
                };
                Generator::Integer {
                    min: if has(&name, &["age"]) { 18 } else { 1 },
                    max,
                }
            } else {
                let scale = mods.get(1).copied().unwrap_or(2).min(4);
                let digits = mods.first().copied().unwrap_or(10);
                let cap = 10f64.powi(digits.saturating_sub(scale).min(9) as i32) - 1.0;
                let (min, max): (f64, f64) = if has(&name, &["salary"]) {
                    (30_000.0, 150_000.0)
                } else if money {
                    (1.0, 1000.0)
                } else {
                    (0.0, 100.0)
                };
                Generator::Decimal {
                    min: min.min(cap),
                    max: max.min(cap),
                    scale,
                }
            }
        }
        "datetime" => match c.base_type.as_str() {
            "date" if has(&name, &["birth", "dob"]) => Generator::Date {
                from: years_ago(70),
                to: years_ago(18),
            },
            "date" => Generator::Date {
                from: years_ago(5),
                to: now.to_string(),
            },
            "timestamp" | "timestamptz" => Generator::Timestamp {
                from: years_ago(1),
                to: now.to_string(),
            },
            _ => {
                return (
                    if c.nullable {
                        Generator::Null
                    } else {
                        Generator::Constant {
                            value: "00:00:00".into(),
                        }
                    },
                    Some("time and interval columns get a fixed value".into()),
                )
            }
        },
        "text" => {
            let short = mods.first().map(|&n| n as usize);
            if (short == Some(2) && has(&name, &["country", "code"]))
                || has(&name, &["country_code", "iso"])
            {
                Generator::CountryCode
            } else if has(&name, &["email", "e_mail"]) {
                Generator::Email
            } else if has(&name, &["first_name", "firstname", "given"]) {
                Generator::FirstName
            } else if has(&name, &["last_name", "lastname", "surname", "family"]) {
                Generator::LastName
            } else if has(&name, &["user_name", "username", "login", "handle"]) {
                Generator::Username
            } else if has(&name, &["phone", "mobile", "tel"]) {
                Generator::Phone
            } else if has(
                &name,
                &["company", "organization", "organisation", "employer"],
            ) {
                Generator::Company
            } else if has(&name, &["city", "town"]) {
                Generator::City
            } else if has(&name, &["country"]) {
                Generator::Country
            } else if has(&name, &["address", "street"]) {
                Generator::StreetAddress
            } else if has(&name, &["zip", "postal", "postcode"]) {
                Generator::PostalCode
            } else if has(&name, &["url", "website", "link"]) {
                Generator::Url
            } else if has(
                &name,
                &["description", "bio", "notes", "comment", "body", "content"],
            ) {
                Generator::Paragraph
            } else if has(&name, &["title", "subject", "summary"]) {
                Generator::Sentence
            } else if name == "name" || has(&name, &["full_name", "fullname", "display_name"]) {
                let t = table.to_lowercase();
                if has(&t, &["product", "item", "sku"]) {
                    Generator::ProductName
                } else if has(
                    &t,
                    &[
                        "compan",
                        "organi",
                        "vendor",
                        "supplier",
                        "customer",
                        "account",
                        "department",
                        "team",
                    ],
                ) {
                    Generator::Company
                } else {
                    Generator::FullName
                }
            } else {
                Generator::Word
            }
        }
        "array" => Generator::Constant { value: "{}".into() },
        _ if c.nullable => {
            return (
                Generator::Null,
                Some(format!("no generator for {}", c.data_type)),
            );
        }
        _ if c.has_default => {
            return (
                Generator::Default,
                Some(format!(
                    "no generator for {}; uses the default",
                    c.data_type
                )),
            );
        }
        _ => {
            return (
                Generator::Null,
                Some(format!(
                    "no generator for {}: choose a constant value",
                    c.data_type
                )),
            )
        }
    };
    (g, None)
}

/// Narrows a generator to the CHECK rules of its column.
pub fn apply_rules(g: Generator, rules: &CheckRules) -> Generator {
    if let Some(values) = &rules.values {
        if !values.is_empty() {
            return Generator::Values {
                values: values.clone(),
            };
        }
    }
    if rules.min.is_none() && rules.max.is_none() {
        return g;
    }
    match g {
        Generator::Integer { mut min, mut max } => {
            if let Some((v, strict)) = rules.min {
                let lo = if strict {
                    v.floor() as i64 + 1
                } else {
                    v.ceil() as i64
                };
                min = min.max(lo);
            }
            if let Some((v, strict)) = rules.max {
                let hi = if strict {
                    v.ceil() as i64 - 1
                } else {
                    v.floor() as i64
                };
                max = max.min(hi);
            }
            if min > max {
                max = min;
            }
            Generator::Integer { min, max }
        }
        Generator::Decimal {
            mut min,
            mut max,
            scale,
        } => {
            let step = 10f64.powi(-(scale as i32));
            if let Some((v, strict)) = rules.min {
                min = min.max(if strict { v + step } else { v });
            }
            if let Some((v, strict)) = rules.max {
                max = max.min(if strict { v - step } else { v });
            }
            if min > max {
                max = min;
            }
            Generator::Decimal { min, max, scale }
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, data_type: &str, base: &str, category: char) -> Column {
        Column {
            name: name.into(),
            data_type: data_type.into(),
            cast_type: data_type.into(),
            base_type: base.into(),
            category,
            nullable: false,
            has_default: false,
            identity: None,
            generated: false,
            enum_values: vec![],
        }
    }

    fn ctx(index: u64) -> RowContext {
        RowContext { index, run_tag: 7 }
    }

    #[test]
    fn same_seed_same_values() {
        let g = Generator::FullName;
        let a: Vec<_> = {
            let mut r = Rng::for_table(42, "s", "t");
            (0..5).map(|i| generate(&g, &mut r, &ctx(i))).collect()
        };
        let b: Vec<_> = {
            let mut r = Rng::for_table(42, "s", "t");
            (0..5).map(|i| generate(&g, &mut r, &ctx(i))).collect()
        };
        assert_eq!(a, b);
        let mut other = Rng::for_table(42, "s", "u");
        assert_ne!(Some(a[0].clone()), Some(generate(&g, &mut other, &ctx(0))));
    }

    #[test]
    fn generators_respect_their_ranges() {
        let mut r = Rng::new(1);
        for i in 0..2000 {
            let v: i64 = generate(&Generator::Integer { min: -3, max: 3 }, &mut r, &ctx(i))
                .unwrap()
                .parse()
                .unwrap();
            assert!((-3..=3).contains(&v));
            let d: f64 = generate(
                &Generator::Decimal {
                    min: 0.01,
                    max: 9.99,
                    scale: 2,
                },
                &mut r,
                &ctx(i),
            )
            .unwrap()
            .parse()
            .unwrap();
            assert!((0.01..=9.99).contains(&d));
            let date = generate(
                &Generator::Date {
                    from: "2020-01-01".into(),
                    to: "2020-01-31".into(),
                },
                &mut r,
                &ctx(i),
            )
            .unwrap();
            assert!(date.starts_with("2020-01-"), "{date}");
        }
        assert_eq!(
            generate(&Generator::Sequence { start: Some(10) }, &mut r, &ctx(5)).unwrap(),
            "15"
        );
        let email = generate(&Generator::Email, &mut r, &ctx(3)).unwrap();
        assert!(email.contains("7000003@example."), "{email}");
        let id = generate(&Generator::Uuid, &mut r, &ctx(0)).unwrap();
        assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
        let ts = generate(
            &Generator::Timestamp {
                from: "2024-05-01".into(),
                to: "2024-05-02".into(),
            },
            &mut r,
            &ctx(0),
        )
        .unwrap();
        assert!(ts.starts_with("2024-05-0") && ts.ends_with("+00"), "{ts}");
    }

    #[test]
    fn uniqueness_and_length() {
        assert_eq!(
            uniquify("Engineering".into(), &ctx(4), None),
            "Engineering-7000004"
        );
        let short = uniquify("Engineering".into(), &ctx(4), Some(10));
        assert_eq!(short.chars().count(), 10);
        assert!(short.ends_with("7000004"));
        assert_eq!(truncate("abcdef".into(), Some(3)), "abc");
        assert_eq!(type_modifiers("numeric(12,2)"), [12, 2]);
        assert_eq!(type_modifiers("character(2)"), [2]);
        assert!(type_modifiers("text").is_empty());
    }

    #[test]
    fn checks_are_interpreted() {
        let cols = ["salary", "status", "a", "b"];
        assert_eq!(
            parse_check("CHECK ((salary > (0)::numeric))", "salary", &cols),
            Some(CheckRules {
                min: Some((0.0, true)),
                max: None,
                values: None
            })
        );
        assert_eq!(
            parse_check(
                "CHECK ((status = ANY (ARRAY['pending'::text, 'it''s'::text])))",
                "status",
                &cols
            )
            .unwrap()
            .values,
            Some(vec!["pending".to_string(), "it's".to_string()])
        );
        assert_eq!(
            parse_check("CHECK (((a >= 1) AND (a <= 5)))", "a", &cols),
            Some(CheckRules {
                min: Some((1.0, false)),
                max: Some((5.0, false)),
                values: None
            })
        );
        assert_eq!(
            parse_check("CHECK ((0 < a))", "a", &cols).unwrap().min,
            Some((0.0, true))
        );
        assert_eq!(parse_check("CHECK ((a < b))", "a", &cols), None);
        assert_eq!(
            parse_check("CHECK ((char_length(status) > 0))", "status", &cols),
            None
        );
        assert_eq!(parse_check("CHECK ((a > 1) OR (a < -1))", "a", &cols), None);
    }

    #[test]
    fn rules_narrow_generators() {
        let g = apply_rules(
            Generator::Decimal {
                min: -5.0,
                max: 10.0,
                scale: 2,
            },
            &CheckRules {
                min: Some((0.0, true)),
                max: None,
                values: None,
            },
        );
        assert_eq!(
            g,
            Generator::Decimal {
                min: 0.01,
                max: 10.0,
                scale: 2
            }
        );
        assert_eq!(
            apply_rules(
                Generator::Integer { min: 1, max: 1000 },
                &CheckRules {
                    min: Some((0.0, false)),
                    max: Some((5.0, true)),
                    values: None
                },
            ),
            Generator::Integer { min: 1, max: 4 }
        );
    }

    #[test]
    fn default_generators_follow_names_and_types() {
        let g = |table: &str, c: Column, unique: bool| default_generator(table, &c, unique).0;
        assert_eq!(
            g("employees", col("email", "text", "text", 'S'), true),
            Generator::Email
        );
        assert_eq!(
            g("employees", col("first_name", "text", "text", 'S'), false),
            Generator::FirstName
        );
        assert_eq!(
            g("customers", col("name", "text", "text", 'S'), false),
            Generator::Company
        );
        assert_eq!(
            g("products", col("name", "text", "text", 'S'), false),
            Generator::ProductName
        );
        assert_eq!(
            g("people", col("name", "text", "text", 'S'), false),
            Generator::FullName
        );
        assert_eq!(
            g(
                "customers",
                col("country", "character(2)", "bpchar", 'S'),
                false
            ),
            Generator::CountryCode
        );
        assert_eq!(
            g("t", col("is_active", "boolean", "bool", 'B'), false),
            Generator::Boolean { true_ratio: 0.5 }
        );
        assert_eq!(
            g(
                "employees",
                col("salary", "numeric(12,2)", "numeric", 'N'),
                false
            ),
            Generator::Decimal {
                min: 30_000.0,
                max: 150_000.0,
                scale: 2
            }
        );
        assert_eq!(
            g("t", col("code", "integer", "int4", 'N'), true),
            Generator::Sequence { start: None }
        );
        let mut id = col("id", "integer", "int4", 'N');
        id.has_default = true;
        assert_eq!(g("t", id, true), Generator::Default);
        let mut e = col("mood", "mood", "mood", 'E');
        e.enum_values = vec!["ok".into()];
        assert_eq!(
            g("t", e, false),
            Generator::Values {
                values: vec!["ok".into()]
            }
        );
    }
}

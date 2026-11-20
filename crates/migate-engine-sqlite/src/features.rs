//! Syntax a migration uses that older SQLite versions reject, checked against the version production runs
//! (`target_version`). Syntax only: a feature that depends on how SQLite was compiled (the JSON functions before
//! 3.38.0, the math functions) is left out, since a version alone cannot say whether production has it.

use crate::sql::{Tok, tokens};

/// A SQLite version as SQLite numbers it: 3.35.5 is 3035005. `3.37` means 3.37.0; a bare `3` names no release.
pub fn version_number(text: &str) -> Option<u32> {
    let mut parts = text.trim().split('.');
    let mut next = || parts.next().map(|p| p.parse::<u32>().ok());
    let major = next()??;
    let minor = next()??;
    let patch = next().unwrap_or(Some(0))?;
    (major == 3 && minor < 1000 && patch < 1000 && next().is_none())
        .then_some(major * 1_000_000 + minor * 1000 + patch)
}

/// `3035005` as `3.35.5`.
pub fn dotted(n: u32) -> String {
    format!("{}.{}.{}", n / 1_000_000, n / 1000 % 1000, n % 1000)
}

/// Each feature: its name, the first version that runs it (sqlite.org/changes.html), and whether a statement uses it.
/// Whether a statement uses a feature.
type Uses = fn(&Statement) -> bool;

const FEATURES: &[(&str, u32, Uses)] = &[
    ("UPSERT (ON CONFLICT ... DO)", 3_024_000, |s| {
        s.after("conflict", |t| s.has_seq(t, &["do"]))
    }),
    (
        "ALTER TABLE ... RENAME COLUMN",
        3_025_000,
        |s| matches!(s.alter_action(), Some(("rename", Some(next))) if !next.eq_ignore_ascii_case("to")),
    ),
    ("window function (OVER)", 3_025_000, |s| {
        s.punct_then_word(')', "over")
    }),
    ("generated column", 3_031_000, |s| {
        s.defines_columns()
            && (s.has_words(&["generated", "always"]) || s.word_then_punct("as", '('))
    }),
    ("iif()", 3_032_000, |s| s.calls(|f| f == "iif")),
    ("RETURNING", 3_035_000, |s| s.has_words(&["returning"])),
    // 3.35.0 added DROP COLUMN; 3.35.5 fixed defects in it that could corrupt the database file.
    (
        "ALTER TABLE ... DROP COLUMN",
        3_035_005,
        |s| matches!(s.alter_action(), Some(("drop", next)) if !next.is_some_and(|n| n.eq_ignore_ascii_case("constraint"))),
    ),
    ("STRICT table", 3_037_000, |s| s.strict_table()),
    ("-> or ->> operator", 3_038_000, |s| s.puncts('-', '>')),
    ("unixepoch()", 3_038_000, |s| s.calls(|f| f == "unixepoch")),
    ("RIGHT or FULL JOIN", 3_039_000, |s| {
        s.has_words(&["right", "join"])
            || s.has_words(&["right", "outer", "join"])
            || s.has_words(&["full", "join"])
            || s.has_words(&["full", "outer", "join"])
    }),
    ("concat(), concat_ws() or string_agg()", 3_044_000, |s| {
        s.calls(|f| matches!(f, "concat" | "concat_ws" | "string_agg"))
    }),
    ("JSONB functions", 3_045_000, |s| {
        s.calls(|f| f.starts_with("jsonb"))
    }),
    ("json_pretty()", 3_046_000, |s| {
        s.calls(|f| f == "json_pretty")
    }),
    (
        "ALTER TABLE ... ALTER COLUMN, ADD CHECK or DROP CONSTRAINT",
        3_053_000,
        |s| match s.alter_action() {
            Some(("alter", _)) => true,
            Some(("add", Some(next))) => {
                next.eq_ignore_ascii_case("constraint") || next.eq_ignore_ascii_case("check")
            }
            Some(("drop", Some(next))) => next.eq_ignore_ascii_case("constraint"),
            _ => false,
        },
    ),
];

/// Features in `sql` that SQLite `target` cannot run: (line, feature, the first version that can).
pub fn newer_than(sql: &str, target: u32) -> Vec<(u32, &'static str, u32)> {
    let mut found = Vec::new();
    for (line, text) in statements(sql) {
        let s = Statement::new(&text);
        for &(name, since, uses) in FEATURES {
            if since > target && uses(&s) {
                found.push((line, name, since));
            }
        }
    }
    found
}

/// One statement's tokens: `top` as the tokenizer gives them (a parenthesised part is one group), `flat` with every
/// group opened, so a feature inside a column list or a call is seen.
struct Statement {
    top: Vec<Tok>,
    flat: Vec<Tok>,
}

impl Statement {
    fn new(text: &str) -> Statement {
        fn open(toks: Vec<Tok>, out: &mut Vec<Tok>) {
            for t in toks {
                if let Tok::Group(inner) = t {
                    out.push(Tok::Punct('('));
                    open(tokens(&inner), out);
                    out.push(Tok::Punct(')'));
                } else {
                    out.push(t);
                }
            }
        }
        let top = tokens(text);
        let mut flat = Vec::new();
        open(top.clone(), &mut flat);
        Statement { top, flat }
    }

    fn word(t: &Tok, w: &str) -> bool {
        matches!(t, Tok::Word(x) if x.eq_ignore_ascii_case(w))
    }

    /// `words` appear one after another (unquoted words only: a quoted name is a name, not a keyword).
    fn has_words(&self, words: &[&str]) -> bool {
        self.flat
            .windows(words.len())
            .any(|w| w.iter().zip(words).all(|(t, x)| Self::word(t, x)))
    }

    fn has_seq(&self, from: usize, words: &[&str]) -> bool {
        self.flat[from..]
            .windows(words.len())
            .any(|w| w.iter().zip(words).all(|(t, x)| Self::word(t, x)))
    }

    /// `check` holds for some position after the word `w`.
    fn after(&self, w: &str, check: impl Fn(usize) -> bool) -> bool {
        self.flat
            .iter()
            .position(|t| Self::word(t, w))
            .is_some_and(|i| check(i + 1))
    }

    fn punct_then_word(&self, p: char, w: &str) -> bool {
        self.flat
            .windows(2)
            .any(|x| matches!(x[0], Tok::Punct(c) if c == p) && Self::word(&x[1], w))
    }

    fn word_then_punct(&self, w: &str, p: char) -> bool {
        self.flat
            .windows(2)
            .any(|x| Self::word(&x[0], w) && matches!(x[1], Tok::Punct(c) if c == p))
    }

    fn puncts(&self, a: char, b: char) -> bool {
        self.flat
            .windows(2)
            .any(|x| matches!((&x[0], &x[1]), (Tok::Punct(p), Tok::Punct(q)) if *p == a && *q == b))
    }

    /// A call of a function whose lower-cased name passes `name`.
    fn calls(&self, name: impl Fn(&str) -> bool) -> bool {
        self.flat.windows(2).any(|x| {
            matches!((&x[0], &x[1]), (Tok::Word(f), Tok::Punct('(')) if name(&f.to_ascii_lowercase()))
        })
    }

    /// `CREATE TABLE`, or `ALTER TABLE ... ADD`: the statements that define columns.
    fn defines_columns(&self) -> bool {
        let w: Vec<String> = self
            .top
            .iter()
            .take(6)
            .filter_map(|t| t.ident().map(str::to_ascii_lowercase))
            .collect();
        let create =
            w.first().is_some_and(|f| f == "create") && w.iter().take(4).any(|x| x == "table");
        create || matches!(self.alter_action(), Some(("add", _)))
    }

    /// `CREATE TABLE name (...) STRICT`: the option follows the column list.
    fn strict_table(&self) -> bool {
        let starts = self.top.first().is_some_and(|t| Self::word(t, "create"));
        let after = self
            .top
            .iter()
            .skip_while(|t| !matches!(t, Tok::Group(_)))
            .skip(1);
        starts
            && after
                .filter_map(|t| match t {
                    Tok::Word(w) => Some(w),
                    _ => None,
                })
                .any(|w| w.eq_ignore_ascii_case("strict"))
    }

    /// In `ALTER TABLE [schema.]name ACTION NEXT ...`: the action, lower-cased, and the word after it.
    fn alter_action(&self) -> Option<(&'static str, Option<&str>)> {
        let t = &self.top;
        if !(t.len() > 3 && Self::word(&t[0], "alter") && Self::word(&t[1], "table")) {
            return None;
        }
        let mut i = 3; // past the table name
        if matches!(t.get(i), Some(Tok::Punct('.'))) {
            i += 2; // schema.name
        }
        let action = match t.get(i)?.ident()?.to_ascii_lowercase().as_str() {
            "rename" => "rename",
            "add" => "add",
            "drop" => "drop",
            "alter" => "alter",
            _ => return None,
        };
        Some((action, t.get(i + 1).and_then(Tok::ident)))
    }
}

/// The statements of `sql`, split at each `;` outside quotes, comments and parentheses, with the line each starts on.
/// A trigger body is split at its inner `;` too, which only makes the reported lines more precise.
fn statements(sql: &str) -> Vec<(u32, String)> {
    let c: Vec<char> = sql.chars().collect();
    let (mut out, mut current, mut begun) = (Vec::new(), String::new(), None::<u32>);
    let (mut line, mut depth, mut i) = (1u32, 0usize, 0usize);
    while i < c.len() {
        let ch = c[i];
        let take = |n: usize, current: &mut String, line: &mut u32| {
            for &x in &c[i..(i + n).min(c.len())] {
                current.push(x);
                if x == '\n' {
                    *line += 1;
                }
            }
        };
        if ch == '-' && c.get(i + 1) == Some(&'-') {
            let end = c[i..]
                .iter()
                .position(|&x| x == '\n')
                .map_or(c.len(), |p| i + p);
            take(end - i, &mut current, &mut line);
            i = end;
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            let end = (i + 2..c.len().saturating_sub(1))
                .find(|&j| c[j] == '*' && c[j + 1] == '/')
                .map_or(c.len(), |j| j + 2);
            take(end - i, &mut current, &mut line);
            i = end;
            continue;
        }
        if !ch.is_whitespace() && begun.is_none() && ch != ';' {
            begun = Some(line);
        }
        if matches!(ch, '\'' | '"' | '`' | '[') {
            let close = if ch == '[' { ']' } else { ch };
            let end = c[i + 1..]
                .iter()
                .position(|&x| x == close)
                .map_or(c.len(), |p| i + 1 + p + 1);
            take(end - i, &mut current, &mut line);
            i = end;
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => {
                if let Some(start) = begun.take() {
                    out.push((start, std::mem::take(&mut current)));
                }
                current.clear();
                i += 1;
                continue;
            }
            _ => {}
        }
        take(1, &mut current, &mut line);
        i += 1;
    }
    if let Some(start) = begun {
        out.push((start, current));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(sql: &str, target: &str) -> Vec<(u32, &'static str)> {
        newer_than(sql, version_number(target).unwrap())
            .into_iter()
            .map(|(l, f, _)| (l, f))
            .collect()
    }

    #[test]
    fn versions_parse_the_way_sqlite_numbers_them() {
        assert_eq!(version_number("3.35.5"), Some(3_035_005));
        assert_eq!(version_number("3.37"), Some(3_037_000));
        assert_eq!(version_number(" 3.8.11 "), Some(3_008_011));
        assert_eq!(dotted(3_035_005), "3.35.5");
        for bad in ["", "3", "4.1", "3.x", "3.1.2.3", "3.1000"] {
            assert_eq!(version_number(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn each_feature_is_found_on_its_own_line() {
        let sql = "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT) STRICT;
ALTER TABLE users DROP COLUMN legacy;
ALTER TABLE users RENAME COLUMN email TO mail;
ALTER TABLE users ADD COLUMN slug TEXT GENERATED ALWAYS AS (lower(mail)) VIRTUAL;
INSERT INTO users (id) VALUES (1) ON CONFLICT (id) DO NOTHING;
UPDATE users SET mail = 'x' RETURNING id;
CREATE VIEW v AS SELECT data ->> '$.a', unixepoch(), iif(1, 2, 3), row_number() OVER (ORDER BY id) FROM t
  RIGHT JOIN u ON 1;
CREATE VIEW w AS SELECT concat(a, b), jsonb('{}'), json_pretty('{}') FROM t;
ALTER TABLE users ALTER COLUMN mail SET NOT NULL;
ALTER TABLE users ADD CONSTRAINT positive CHECK (id > 0);
ALTER TABLE users DROP CONSTRAINT positive;";
        let got = found(sql, "3.20");
        let expected = [
            (1, "STRICT table"),
            (2, "ALTER TABLE ... DROP COLUMN"),
            (3, "ALTER TABLE ... RENAME COLUMN"),
            (4, "generated column"),
            (5, "UPSERT (ON CONFLICT ... DO)"),
            (6, "RETURNING"),
            (7, "window function (OVER)"),
            (7, "iif()"),
            (7, "-> or ->> operator"),
            (7, "unixepoch()"),
            (7, "RIGHT or FULL JOIN"),
            (9, "concat(), concat_ws() or string_agg()"),
            (9, "JSONB functions"),
            (9, "json_pretty()"),
            (
                10,
                "ALTER TABLE ... ALTER COLUMN, ADD CHECK or DROP CONSTRAINT",
            ),
            (
                11,
                "ALTER TABLE ... ALTER COLUMN, ADD CHECK or DROP CONSTRAINT",
            ),
            (
                12,
                "ALTER TABLE ... ALTER COLUMN, ADD CHECK or DROP CONSTRAINT",
            ),
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn a_target_new_enough_finds_nothing_and_the_threshold_is_exact() {
        assert!(found("ALTER TABLE t DROP COLUMN c;", "3.54.1").is_empty());
        assert_eq!(
            found("ALTER TABLE t DROP COLUMN c;", "3.35.4"),
            [(1, "ALTER TABLE ... DROP COLUMN")]
        );
        assert!(found("ALTER TABLE t DROP COLUMN c;", "3.35.5").is_empty());
    }

    #[test]
    fn old_syntax_that_looks_alike_is_not_flagged() {
        let sql = "ALTER TABLE users RENAME TO people;
ALTER TABLE main.people ADD COLUMN age INTEGER CHECK (age > 0);
CREATE TABLE copy AS SELECT CAST(id AS TEXT) AS id FROM people;
CREATE TABLE k (id INTEGER PRIMARY KEY ON CONFLICT REPLACE, v TEXT) WITHOUT ROWID;
-- a comment saying RETURNING, DROP COLUMN and ->> is not SQL
SELECT 'RETURNING ->> concat(' AS s, json_extract(d, '$.a'), \"right\" FROM t;
CREATE TRIGGER tr AFTER INSERT ON people BEGIN UPDATE people SET age = 1; END;";
        assert_eq!(found(sql, "3.20"), []);
    }

    #[test]
    fn a_statement_s_line_is_where_it_starts_after_comments_and_blank_lines() {
        let sql = "-- header\n\n/* block\ncomment */\nALTER TABLE t\n  DROP COLUMN c;\n\nSELECT 1 RETURNING x";
        assert_eq!(
            found(sql, "3.20"),
            [(5, "ALTER TABLE ... DROP COLUMN"), (8, "RETURNING")]
        );
    }
}

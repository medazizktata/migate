//! Hashing migration files for the ledger.
//!
//! Two digests per file:
//! - `sha256`: the bytes with a UTF-8 BOM removed and CRLF/CR turned into LF, so a checkout on Windows does not look like
//!   an edit (Prisma had false mismatches from line endings; Flyway strips the BOM).
//! - `code_sha256`: the same text with comments removed and whitespace outside strings collapsed, so an edit that only
//!   touches comments or layout can be told apart from a real one (rule MG007 versus MG001).
//!
//! The comment stripper errs toward strictness: whatever it cannot be sure is a comment counts as code, so a real edit
//! is never downgraded to "comment-only".

use sha2::{Digest, Sha256};

/// The comment syntax of a migration file, chosen from its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// `--` and `/* */`; strings `'…'` (with `''`), identifiers `"…"`, Postgres `$tag$…$tag$` bodies kept verbatim.
    /// `#` is not treated as a comment: it is MySQL's, but an operator in Postgres, so a `#` edit counts as code.
    Sql,
    /// JavaScript and TypeScript: `//` and `/* */`; strings `'…'`, `"…"` and template literals.
    /// shortcut: a regex literal containing `//` or a quote is not recognised; upgrade to a real tokenizer if one appears.
    CLike,
    /// Python and Ruby: `#`; strings `'…'`, `"…"` and Python triple quotes.
    Hash,
    /// Anything else: no comment is recognised, so every edit counts as code.
    Unknown,
}

impl Lang {
    pub fn from_path(path: &str) -> Lang {
        match path
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("sql") => Lang::Sql,
            Some("ts" | "mts" | "cts" | "js" | "mjs" | "cjs") => Lang::CLike,
            Some("py" | "rb") => Lang::Hash,
            _ => Lang::Unknown,
        }
    }
}

/// The ledger's two digests of one file, as lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digests {
    pub sha256: String,
    pub code_sha256: String,
}

/// SHA-256 of the bytes exactly as they are, the way a project's own tooling records a migration.
pub fn raw_sha256(bytes: &[u8]) -> String {
    hex_sha256(bytes)
}

pub fn digests(bytes: &[u8], lang: Lang) -> Digests {
    let normalised = normalise(bytes);
    let sha256 = hex_sha256(&normalised);
    let code_sha256 = match (lang, std::str::from_utf8(&normalised)) {
        (Lang::Unknown, _) | (_, Err(_)) => sha256.clone(), // not text we can read: every edit counts as code
        (_, Ok(text)) => hex_sha256(code_only(text, lang).as_bytes()),
    };
    Digests {
        sha256,
        code_sha256,
    }
}

/// Remove a UTF-8 BOM and turn CRLF and lone CR into LF, on raw bytes, so invalid UTF-8 still hashes exactly.
pub fn normalise(bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(b'\n');
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    out
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The text with comments removed and every run of whitespace outside strings collapsed to one space.
pub fn code_only(text: &str, lang: Lang) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut pending_space = false;
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, ch)| c.get(i + k) == Some(&ch))
    };

    while i < c.len() {
        // Comments become a single space, so `a/*x*/b` and `a b` compare equal but `ab` does not.
        let line_comment = match lang {
            Lang::Sql => at(i, "--"),
            Lang::CLike => at(i, "//"),
            Lang::Hash => c[i] == '#',
            Lang::Unknown => false,
        };
        if line_comment {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            pending_space = true;
            continue;
        }
        if matches!(lang, Lang::Sql | Lang::CLike) && at(i, "/*") {
            match (i + 2..c.len().saturating_sub(1)).find(|&j| c[j] == '*' && c[j + 1] == '/') {
                Some(end) => i = end + 2,
                // Unterminated: treat the rest as code, never as a comment.
                None => {
                    out.extend(&c[i..]);
                    break;
                }
            }
            pending_space = true;
            continue;
        }
        if c[i].is_whitespace() {
            pending_space = true;
            i += 1;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;

        // Strings are copied verbatim, comment markers and whitespace inside them included.
        if let Some(end) = string_end(&c, i, lang) {
            out.extend(&c[i..end]);
            i = end;
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// If a string or verbatim body starts at `i`, the index just past its end (or the text's end if unterminated).
fn string_end(c: &[char], i: usize, lang: Lang) -> Option<usize> {
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, ch)| c.get(i + k) == Some(&ch))
    };
    if lang == Lang::Hash {
        for q in ["\"\"\"", "'''"] {
            if at(i, q) {
                let mut j = i + 3;
                while j < c.len() && !at(j, q) {
                    j += if c[j] == '\\' { 2 } else { 1 };
                }
                return Some((j + 3).min(c.len()));
            }
        }
    }
    if lang == Lang::Sql && c[i] == '$' {
        // A Postgres dollar quote: $$ or $tag$, where the tag does not start with a digit ($1 is a parameter).
        let tag_end = (i + 1..c.len()).find(|&j| !(c[j].is_alphanumeric() || c[j] == '_'))?;
        let tag: String = c[i..=tag_end].iter().collect();
        if c[tag_end] != '$' || c.get(i + 1).is_some_and(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let body = tag_end + 1;
        let close = (body..c.len()).find(|&j| at(j, &tag));
        return Some(close.map_or(c.len(), |j| j + tag.chars().count()));
    }
    let quote = c[i];
    let is_quote = match lang {
        Lang::Sql => quote == '\'' || quote == '"',
        Lang::CLike => quote == '\'' || quote == '"' || quote == '`',
        Lang::Hash => quote == '\'' || quote == '"',
        Lang::Unknown => false,
    };
    if !is_quote {
        return None;
    }
    let mut j = i + 1;
    while j < c.len() {
        if c[j] == '\\' && lang != Lang::Sql {
            j += 2;
            continue;
        }
        // A doubled quote (SQL's escape) needs no case of its own: ending the string there and starting the next one
        // at once keeps the same characters inside strings.
        if c[j] == quote {
            return Some(j + 1);
        }
        j += 1;
    }
    Some(c.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_raw_digest_is_sha256_of_the_bytes_as_they_are() {
        assert_eq!(
            raw_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn every_string_form_keeps_its_comment_markers_and_ends_where_it_ends() {
        // Each case: a string holding comment-like text, then a real comment. The string survives whole, the comment
        // goes; a wrong string end would either eat the comment into code or cut the string into a "comment".
        let cases = [
            (
                Lang::Sql,
                r##"SELECT 'it''s -- x' -- c"##,
                r##"SELECT 'it''s -- x'"##,
            ),
            (
                Lang::Sql,
                r##"SELECT "a -- b" -- c"##,
                r##"SELECT "a -- b""##,
            ),
            (
                Lang::Sql,
                r##"SELECT $$ -- x $$ -- c"##,
                r##"SELECT $$ -- x $$"##,
            ),
            // Each case below ends a string or comment right before a comment marker, where a wrong end shows.
            (Lang::Sql, r##"SELECT /*/ x */ 1"##, r##"SELECT 1"##),
            (Lang::Sql, r##"SELECT '' -- c"##, r##"SELECT ''"##),
            (Lang::Sql, r##"SELECT /* a*b/c */ 1"##, r##"SELECT 1"##),
            (Lang::Sql, r##"SELECT $1$ -- c"##, r##"SELECT $1$"##),
            (Lang::Sql, r##"SELECT $a -- c"##, r##"SELECT $a"##),
            (
                Lang::Sql,
                r##"SELECT $$$ x $$ -- c"##,
                r##"SELECT $$$ x $$"##,
            ),
            (
                Lang::Sql,
                r##"SELECT $$ open -- c"##,
                r##"SELECT $$ open -- c"##,
            ),
            (
                Lang::Hash,
                r##"x = """a " # b""" # c"##,
                r##"x = """a " # b""""##,
            ),
            (Lang::Hash, r##"x = """open # c"##, r##"x = """open # c"##),
            (Lang::Hash, r##"x = 'a\\' # c"##, r##"x = 'a\\'"##),
            (Lang::CLike, r##"f("a\\") // c"##, r##"f("a\\")"##),
            (
                Lang::Sql,
                r##"SELECT $fn$ /* x */ $fn$ -- c"##,
                r##"SELECT $fn$ /* x */ $fn$"##,
            ),
            (Lang::Sql, r##"SELECT $1 -- c"##, r##"SELECT $1"##),
            (
                Lang::Sql,
                r##"SELECT 'open -- x"##,
                r##"SELECT 'open -- x"##,
            ),
            (
                Lang::Hash,
                r##"x = """# a \""" # b""" # c"##,
                r##"x = """# a \""" # b""""##,
            ),
            (Lang::Hash, r##"x = '''# a''' # c"##, r##"x = '''# a'''"##),
            (Lang::Hash, r##"x = 'a\' # b' # c"##, r##"x = 'a\' # b'"##),
            (Lang::CLike, r##"f(`// a`) // c"##, r##"f(`// a`)"##),
            (Lang::CLike, r##"f("a\" // b") // c"##, r##"f("a\" // b")"##),
        ];
        for (lang, text, code) in cases {
            assert_eq!(code_only(text, lang), code, "{text}");
        }
    }

    fn same_code(a: &str, b: &str, lang: Lang) -> bool {
        digests(a.as_bytes(), lang).code_sha256 == digests(b.as_bytes(), lang).code_sha256
    }

    #[test]
    fn line_endings_and_bom_do_not_change_the_hash() {
        let lf = "ALTER TABLE a ADD b int;\nSELECT 1;\n";
        let crlf = "\u{feff}ALTER TABLE a ADD b int;\r\nSELECT 1;\r\n";
        assert_eq!(
            digests(lf.as_bytes(), Lang::Sql).sha256,
            digests(crlf.as_bytes(), Lang::Sql).sha256
        );
    }

    #[test]
    fn a_one_character_edit_changes_both_hashes() {
        let a = digests(b"ALTER TABLE a ADD b int;", Lang::Sql);
        let b = digests(b"ALTER TABLE a ADD c int;", Lang::Sql);
        assert_ne!(a.sha256, b.sha256);
        assert_ne!(a.code_sha256, b.code_sha256);
    }

    #[test]
    fn comment_and_layout_edits_keep_the_code_hash() {
        assert!(same_code(
            "ALTER TABLE a ADD b int; -- old",
            "-- new note\nALTER TABLE a\n  ADD b int;",
            Lang::Sql
        ));
        assert!(same_code("SELECT /* x */ 1", "SELECT /* y */ 1", Lang::Sql));
        assert!(same_code(
            "await db.run(sql`x`) // a",
            "// b\nawait db.run(sql`x`)",
            Lang::CLike
        ));
        assert!(same_code(
            "op.add_column('t', c)  # a",
            "# b\nop.add_column('t', c)",
            Lang::Hash
        ));
    }

    #[test]
    fn comment_markers_inside_strings_are_code() {
        // A `--` inside a string literal is data: editing after it is a real edit.
        assert!(!same_code(
            "INSERT INTO t VALUES ('a -- b')",
            "INSERT INTO t VALUES ('a -- c')",
            Lang::Sql
        ));
        assert!(!same_code(
            "x = '# not a comment 1'",
            "x = '# not a comment 2'",
            Lang::Hash
        ));
        assert!(!same_code(
            "const s = \"// 1\"",
            "const s = \"// 2\"",
            Lang::CLike
        ));
        assert!(!same_code(
            "q = \"\"\"a # 1\"\"\"",
            "q = \"\"\"a # 2\"\"\"",
            Lang::Hash
        ));
        assert!(!same_code(
            "SELECT 'it''s -- 1'",
            "SELECT 'it''s -- 2'",
            Lang::Sql
        ));
    }

    #[test]
    fn postgres_function_bodies_are_verbatim() {
        // Comments inside $$ … $$ are part of the stored function, so changing them is a real edit.
        let a = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; -- one\n $$ LANGUAGE sql;";
        let b = "CREATE FUNCTION f() RETURNS int AS $$ SELECT 1; -- two\n $$ LANGUAGE sql;";
        assert!(!same_code(a, b, Lang::Sql));
        let c = "CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; -- one\n $body$ LANGUAGE sql;";
        let d = "CREATE FUNCTION f() RETURNS int AS $body$ SELECT 1; -- two\n $body$ LANGUAGE sql;";
        assert!(!same_code(c, d, Lang::Sql));
        // $1 is a parameter, not a quote: the comment after it is still a comment.
        assert!(same_code("SELECT $1 -- a", "SELECT $1 -- b", Lang::Sql));
    }

    #[test]
    fn sql_hash_is_code_not_a_comment() {
        // `#` is an operator in Postgres, so it is never stripped.
        assert!(!same_code("SELECT 1 # 2", "SELECT 1 # 3", Lang::Sql));
    }

    #[test]
    fn invalid_utf8_edits_are_never_comment_only() {
        let a = digests(b"SELECT 1 -- \xff", Lang::Sql);
        let b = digests(b"SELECT 1 -- \xfe", Lang::Sql);
        assert_ne!(a.sha256, b.sha256);
        assert_ne!(a.code_sha256, b.code_sha256);
    }

    #[test]
    fn unknown_files_have_no_comment_only_edits() {
        assert!(!same_code("a -- 1", "a -- 2", Lang::Unknown));
    }

    #[test]
    fn unterminated_block_comment_counts_as_code() {
        assert!(!same_code("SELECT 1 /* a", "SELECT 1 /* b", Lang::Sql));
    }

    #[test]
    fn language_from_extension() {
        assert_eq!(Lang::from_path("migrations/0001_init.SQL"), Lang::Sql);
        assert_eq!(
            Lang::from_path("migrations/20261118_232700_x.ts"),
            Lang::CLike
        );
        assert_eq!(
            Lang::from_path("db/migrate/20260101000000_x.rb"),
            Lang::Hash
        );
        assert_eq!(
            Lang::from_path("shop/migrations/0001_initial.py"),
            Lang::Hash
        );
        assert_eq!(Lang::from_path("V1__init.xml"), Lang::Unknown);
    }
}

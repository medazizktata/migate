//! Statements in a migration that delete every row of a table: `DELETE` with no `WHERE`, and `TRUNCATE` (MG107). Read
//! from the migration's text, so the loss is seen even when the sandbox's database holds no rows in that table: MG105
//! counts rows, and an empty table loses none.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Name(String),
    Punct(char),
}

impl Tok {
    fn is(&self, kw: &str) -> bool {
        matches!(self, Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }

    fn name(&self) -> Option<&str> {
        match self {
            Tok::Word(w) | Tok::Name(w) => Some(w),
            Tok::Punct(_) => None,
        }
    }
}

/// Each table a statement in `sql` empties, as `(line, table as written)`. A table the same text creates before
/// emptying it is left out: it holds no rows from before. Quoted text, dollar-quoted bodies, and the bodies of
/// triggers and functions (`BEGIN ... END` in a `CREATE` statement) run later or never, so they do not count.
/// shortcut: a DELETE inside a PostgreSQL `DO $$ ... $$` block runs during the migration but is not read; read the
/// block's body when one is seen in practice.
pub fn emptied_tables(sql: &str) -> Vec<(u32, String)> {
    let mut created: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for statement in statements(&lex(sql)) {
        let words: Vec<&Tok> = statement.iter().map(|(_, t, _)| t).collect();
        let at = |i: usize| words.get(i).copied();
        let depth = |i: usize| statement.get(i).map_or(0, |(_, _, d)| *d);
        if at(0).is_some_and(|t| t.is("CREATE")) {
            let mut i = 1;
            while at(i).is_some_and(|t| ["TEMP", "TEMPORARY", "UNLOGGED"].iter().any(|k| t.is(k))) {
                i += 1;
            }
            // `IF NOT EXISTS` may meet a table that is already there, rows and all: not a new one.
            if at(i).is_some_and(|t| t.is("TABLE"))
                && !at(i + 1).is_some_and(|t| t.is("IF"))
                && let Some((name, _)) = name_at(&words, i + 1)
            {
                created.push(plain(&name));
            }
            continue;
        }
        let new = |name: &str| created.contains(&plain(name));
        if at(0).is_some_and(|t| t.is("TRUNCATE")) {
            let mut i = 1;
            if at(i).is_some_and(|t| t.is("TABLE")) {
                i += 1;
            }
            loop {
                if at(i).is_some_and(|t| t.is("ONLY")) {
                    i += 1;
                }
                let Some((name, next)) = name_at(&words, i) else {
                    break;
                };
                if !new(&name) {
                    out.push((statement[0].0, name));
                }
                i = next;
                if at(i) == Some(&Tok::Punct('*')) {
                    i += 1;
                }
                if at(i) != Some(&Tok::Punct(',')) {
                    break;
                }
                i += 1;
            }
            continue;
        }
        if !at(0).is_some_and(|t| t.is("DELETE") || t.is("WITH")) {
            continue;
        }
        // Each DELETE in the statement, at the top or inside a WITH clause; its WHERE is the one in its own group.
        for i in (0..words.len())
            .filter(|&i| words[i].is("DELETE") && at(i + 1).is_some_and(|t| t.is("FROM")))
        {
            let d = depth(i);
            let end = (i + 1..words.len())
                .find(|&j| depth(j) < d)
                .unwrap_or(words.len());
            let filtered = (i..end).any(|j| depth(j) == d && words[j].is("WHERE"));
            let from = if at(i + 2).is_some_and(|t| t.is("ONLY")) {
                i + 3
            } else {
                i + 2
            };
            if let Some((name, _)) = name_at(&words, from)
                && !filtered
                && !new(&name)
            {
                out.push((statement[i].0, name));
            }
        }
    }
    out
}

/// A possibly qualified name starting at `i` (`a`, `a.b`, `"A"."b"`), and the index after it.
fn name_at(words: &[&Tok], i: usize) -> Option<(String, usize)> {
    let mut parts = vec![words.get(i)?.name()?.to_string()];
    let mut j = i + 1;
    while words.get(j) == Some(&&Tok::Punct('.')) {
        parts.push(words.get(j + 1)?.name()?.to_string());
        j += 2;
    }
    Some((parts.join("."), j))
}

/// A table's name to compare with: lower case, without the default schema (`public`, SQLite's `main`).
fn plain(name: &str) -> String {
    let name = name.to_lowercase();
    ["public.", "main."]
        .iter()
        .find_map(|schema| name.strip_prefix(schema))
        .map_or_else(|| name.clone(), str::to_string)
}

/// Top-level statements, each token with its line and its parenthesis depth. In a `CREATE` statement a `BEGIN` opens
/// a body (a SQLite trigger, a SQL-standard function) whose `;`s do not end the statement; `CASE` and `BEGIN` nest
/// inside it and `END` closes one level.
fn statements(tokens: &[(u32, Tok)]) -> Vec<Vec<(u32, Tok, usize)>> {
    let (mut out, mut current) = (Vec::new(), Vec::new());
    let (mut depth, mut body) = (0usize, 0usize);
    for (line, t) in tokens {
        match t {
            Tok::Punct('(') => depth += 1,
            Tok::Punct(')') => depth = depth.saturating_sub(1),
            Tok::Punct(';') if depth == 0 && body == 0 => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        let creating = current
            .first()
            .is_some_and(|(_, f, _): &(u32, Tok, usize)| f.is("CREATE"));
        if creating && ((t.is("BEGIN") && (body > 0 || depth == 0)) || (body > 0 && t.is("CASE"))) {
            body += 1;
        } else if body > 0 && t.is("END") {
            body -= 1;
        }
        current.push((*line, t.clone(), depth));
    }
    out.push(current);
    out.retain(|s| !s.is_empty());
    out
}

/// Words, quoted names and punctuation with their line; comments, strings and dollar-quoted text are dropped.
fn lex(sql: &str) -> Vec<(u32, Tok)> {
    let c: Vec<char> = sql.chars().collect();
    let (mut i, mut line, mut out) = (0usize, 1u32, Vec::new());
    let ident = |ch: char| ch.is_alphanumeric() || ch == '_' || ch == '$';
    // Moves `i` to `end`, counting the lines passed.
    let skip = |i: &mut usize, line: &mut u32, end: usize| {
        let end = end.min(c.len());
        *line += u32::try_from(c[*i..end].iter().filter(|&&ch| ch == '\n').count()).unwrap_or(0);
        *i = end;
    };
    while i < c.len() {
        let ch = c[i];
        let starts_token = i == 0 || !ident(c[i - 1]);
        if ch == '-' && c.get(i + 1) == Some(&'-') {
            let end = (i..c.len()).find(|&j| c[j] == '\n').unwrap_or(c.len());
            skip(&mut i, &mut line, end);
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            let (mut j, mut nest) = (i + 2, 1);
            while j < c.len() && nest > 0 {
                if c[j] == '/' && c.get(j + 1) == Some(&'*') {
                    nest += 1;
                    j += 1;
                } else if c[j] == '*' && c.get(j + 1) == Some(&'/') {
                    nest -= 1;
                    j += 1;
                }
                j += 1;
            }
            skip(&mut i, &mut line, j);
        } else if ch == '\'' {
            // E'...' takes backslash escapes; the E was read as a word just before.
            let escapes = matches!(out.last(), Some((_, Tok::Word(w))) if w.eq_ignore_ascii_case("E"))
                && i > 0
                && matches!(c[i - 1], 'E' | 'e');
            if escapes {
                out.pop();
            }
            let mut j = i + 1;
            while j < c.len() {
                if (escapes && c[j] == '\\') || (c[j] == '\'' && c.get(j + 1) == Some(&'\'')) {
                    j += 2;
                } else if c[j] == '\'' {
                    break;
                } else {
                    j += 1;
                }
            }
            skip(&mut i, &mut line, j + 1);
        } else if ch == '"' || ch == '`' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < c.len() {
                if c[j] == ch && c.get(j + 1) == Some(&ch) {
                    s.push(ch);
                    j += 2;
                } else if c[j] == ch {
                    break;
                } else {
                    s.push(c[j]);
                    j += 1;
                }
            }
            let at = line;
            skip(&mut i, &mut line, j + 1);
            out.push((at, Tok::Name(s)));
        } else if ch == '$'
            && starts_token
            && let Some(tag_end) = dollar_tag(&c, i)
        {
            let tag: String = c[i..=tag_end].iter().collect();
            let body = tag_end + 1;
            let close = (body..c.len())
                .find(|&j| c[j..].starts_with(&tag.chars().collect::<Vec<_>>()))
                .map_or(c.len(), |j| j + tag.chars().count());
            skip(&mut i, &mut line, close);
        } else if ident(ch) {
            let start = i;
            while i < c.len() && ident(c[i]) {
                i += 1;
            }
            out.push((line, Tok::Word(c[start..i].iter().collect())));
        } else {
            if ch == '\n' {
                line += 1;
            } else if !ch.is_whitespace() {
                out.push((line, Tok::Punct(ch)));
            }
            i += 1;
        }
    }
    out
}

/// The index of the closing `$` of a dollar-quote tag opening at `i` (`$$`, `$body$`); `None` for `$1` and the like.
fn dollar_tag(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < c.len() && (c[j].is_alphanumeric() || c[j] == '_') {
        if j == i + 1 && c[j].is_ascii_digit() {
            return None;
        }
        j += 1;
    }
    (c.get(j) == Some(&'$')).then_some(j)
}

#[cfg(test)]
mod tests {
    use super::emptied_tables;

    fn found(sql: &str) -> Vec<(u32, String)> {
        emptied_tables(sql)
    }

    #[test]
    fn a_delete_without_where_and_a_truncate_empty_their_tables() {
        assert_eq!(
            found("ALTER TABLE t ADD c int;\n\ndelete from public.prediction_schema_snapshots;\n"),
            [(3, "public.prediction_schema_snapshots".to_string())]
        );
        assert_eq!(
            found("TRUNCATE TABLE ONLY a, \"B\" * CASCADE;"),
            [(1, "a".to_string()), (1, "B".to_string())]
        );
        assert_eq!(
            found("WITH gone AS (SELECT 1) DELETE FROM t RETURNING *;"),
            [(1, "t".to_string())]
        );
    }

    #[test]
    fn a_filtered_delete_or_one_in_text_that_does_not_run_is_not_counted() {
        assert!(found("DELETE FROM t WHERE id IN (SELECT id FROM u);").is_empty());
        assert!(found("DELETE FROM t USING u WHERE t.id = u.id;").is_empty());
        assert!(found("-- DELETE FROM t;\n/* TRUNCATE t; /* nested */ */ SELECT 1;").is_empty());
        assert!(
            found("INSERT INTO log VALUES ('DELETE FROM t;', E'it\\'s; DELETE FROM t');")
                .is_empty()
        );
        assert!(
            found(
                "CREATE FUNCTION f() RETURNS void AS $body$ SELECT 1; DELETE FROM t; $body$ LANGUAGE sql;\n\
             CREATE FUNCTION g() RETURNS int AS $$ SELECT $1 $$ LANGUAGE sql;"
            )
            .is_empty()
        );
        assert!(found(
            "CREATE TRIGGER tr AFTER INSERT ON a BEGIN\n  SELECT CASE WHEN 1 THEN 2 END;\n  DELETE FROM b;\nEND;\nSELECT 1;"
        )
        .is_empty());
        assert!(found("SELECT a[1] FROM t;").is_empty());
    }

    #[test]
    fn the_line_reported_is_the_statement_s_own_after_text_spanning_lines() {
        // Comments, strings, quoted names and dollar-quoted bodies that span lines are counted, and only once.
        let sql = "-- one\n/* two\nthree */ SELECT 'four\nfive' AS \"six\nseven\";\nSELECT $b$ eight\nnine $b$;\nDELETE FROM t;";
        assert_eq!(found(sql), [(8, "t".to_string())]);
        assert_eq!(found("\n\n  TRUNCATE a;"), [(3, "a".to_string())]);
        assert_eq!(
            found("SELECT 1;\nWITH g AS (\n  DELETE FROM t RETURNING *\n) SELECT 1;"),
            [(3, "t".to_string())]
        );
    }

    #[test]
    fn the_lexer_ends_comments_strings_and_quotes_where_the_database_does() {
        // A nested comment ends at its own close, not the first one.
        assert_eq!(
            found("/* a /* b */ DELETE FROM x; */ DELETE FROM y;"),
            [(1, "y".to_string())]
        );
        // Without E, a backslash is an ordinary character: the string ends at the next quote.
        assert_eq!(
            found("SELECT 'a\\'; DELETE FROM t;"),
            [(1, "t".to_string())]
        );
        // With e (any case), it escapes the quote, so the DELETE is inside the string.
        assert!(found("SELECT e'a\\'; DELETE FROM t;';").is_empty());
        // A word merely ending in E is not the E prefix.
        assert_eq!(
            found("SELECT CASE WHEN 1 THEN 'x\\' END; DELETE FROM t;"),
            [(1, "t".to_string())]
        );
        // A doubled quote inside a quoted name or a string stays inside it.
        assert_eq!(
            found("DELETE FROM \"we\"\"ird\";"),
            [(1, "we\"ird".to_string())]
        );
        assert!(found("SELECT 'it''s; DELETE FROM t;';").is_empty());
        assert_eq!(found("DELETE FROM `t`;"), [(1, "t".to_string())]);
        // A dollar tag may hold digits after its first character; `$1` is a parameter, not a quote.
        assert!(found("SELECT $a1$ x; DELETE FROM t; $a1$;").is_empty());
        assert_eq!(found("SELECT $1; DELETE FROM t;"), [(1, "t".to_string())]);
        assert!(found("SELECT $$ x; DELETE FROM t; $$;").is_empty());
        // A minus sign is not a comment, and E with a space before the quote is a word, not the E prefix.
        assert_eq!(
            found("SELECT 1 - 2; DELETE FROM t;"),
            [(1, "t".to_string())]
        );
        assert_eq!(
            found("SELECT E 'x\\'; DELETE FROM t;"),
            [(1, "t".to_string())]
        );
        // Text left open at the end, or no final semicolon: no crash, and the last statement is still read.
        for open in [
            "SELECT 'abc",
            "DELETE FROM \"abc",
            "/* never closed",
            "SELECT $q$ open",
        ] {
            let _ = found(open);
        }
        assert_eq!(
            found("ALTER TABLE t ADD c int;\nDELETE FROM t"),
            [(2, "t".to_string())]
        );
        // A word may hold a dollar sign: it does not open a quote.
        assert_eq!(found("SELECT a$b; DELETE FROM t;"), [(1, "t".to_string())]);
    }

    #[test]
    fn begin_case_and_end_open_a_body_only_inside_a_create() {
        // A transaction's BEGIN and END are statements of their own.
        assert_eq!(
            found("BEGIN;\nDELETE FROM t;\nEND;"),
            [(2, "t".to_string())]
        );
        // CASE ... END in a view is an expression, not a body: the next statement is read.
        assert_eq!(
            found("CREATE VIEW v AS SELECT CASE WHEN a THEN 1 END FROM x;\nDELETE FROM t;"),
            [(2, "t".to_string())]
        );
        // A trigger body with a CASE ... END inside ends at its own END.
        assert_eq!(
            found(
                "CREATE TRIGGER r AFTER INSERT ON a BEGIN SELECT CASE WHEN 1 THEN 2 END; DELETE FROM b; END;\nTRUNCATE c;"
            ),
            [(2, "c".to_string())]
        );
    }

    #[test]
    fn names_and_keywords_around_the_table() {
        assert!(found("CREATE TEMP TABLE t (a int);\nDELETE FROM t;").is_empty());
        assert!(found("CREATE TEMPORARY UNLOGGED TABLE t (a int);\nDELETE FROM t;").is_empty());
        assert_eq!(found("DELETE FROM ONLY s.t;"), [(1, "s.t".to_string())]);
        assert_eq!(found("DELETE FROM db.s.t;"), [(1, "db.s.t".to_string())]);
        assert_eq!(
            found("TRUNCATE ONLY a, ONLY b *, c RESTART IDENTITY;"),
            [
                (1, "a".to_string()),
                (1, "b".to_string()),
                (1, "c".to_string())
            ]
        );
        // The DELETE's own WHERE after a subquery in its USING still filters it.
        assert!(found("DELETE FROM t USING (SELECT 1 AS a) s WHERE t.a = s.a;").is_empty());
        // A WHERE in a group the DELETE does not own does not filter it.
        assert_eq!(
            found("DELETE FROM t USING (SELECT 1 WHERE true) s;"),
            [(1, "t".to_string())]
        );
    }

    #[test]
    fn only_a_table_this_migration_really_created_is_exempt() {
        // Another schema's table of the same name, and one that may already exist, keep their rows.
        assert_eq!(
            found("CREATE TABLE tmp.t (a int);\nDELETE FROM public.t;"),
            [(2, "public.t".to_string())]
        );
        assert_eq!(
            found("CREATE TABLE IF NOT EXISTS t (a int);\nDELETE FROM t;"),
            [(2, "t".to_string())]
        );
        assert!(found("CREATE TABLE t (a int);\nDELETE FROM public.t;").is_empty());
    }

    #[test]
    fn a_delete_inside_a_with_clause_is_read_too() {
        assert_eq!(
            found("WITH gone AS (DELETE FROM t RETURNING *) SELECT count(*) FROM gone;"),
            [(1, "t".to_string())]
        );
        assert!(found("WITH gone AS (DELETE FROM t WHERE a = 1 RETURNING *) SELECT 1;").is_empty());
        // The outer query's WHERE filters what it reads, not what the DELETE inside removed.
        assert_eq!(
            found("WITH gone AS (DELETE FROM t RETURNING *) SELECT * FROM gone WHERE a > 0;"),
            [(1, "t".to_string())]
        );
    }

    #[test]
    fn a_table_the_same_migration_creates_holds_no_rows_from_before() {
        assert!(
            found("CREATE TABLE s.t (a int);\nDELETE FROM s.t;\nTRUNCATE TABLE S.T;").is_empty()
        );
        // A statement after a trigger body is read again.
        assert_eq!(
            found("CREATE TRIGGER tr AFTER INSERT ON a BEGIN DELETE FROM b; END;\nDELETE FROM a;"),
            [(2, "a".to_string())]
        );
    }
}

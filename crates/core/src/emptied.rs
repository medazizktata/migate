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
        let top = |i: usize| statement.get(i).is_some_and(|(_, _, depth)| *depth == 0);
        if at(0).is_some_and(|t| t.is("CREATE")) {
            let mut i = 1;
            while at(i).is_some_and(|t| ["TEMP", "TEMPORARY", "UNLOGGED"].iter().any(|k| t.is(k))) {
                i += 1;
            }
            if at(i).is_some_and(|t| t.is("TABLE")) {
                i += 1;
                if at(i).is_some_and(|t| t.is("IF")) {
                    i += 3;
                }
                if let Some((name, _)) = name_at(&words, i) {
                    created.push(last(&name));
                }
            }
            continue;
        }
        let new = |name: &str| created.contains(&last(name));
        let first = match at(0) {
            Some(t) if t.is("WITH") => (0..words.len())
                .find(|&i| top(i) && (words[i].is("DELETE") || words[i].is("TRUNCATE"))),
            Some(_) => Some(0),
            None => None,
        };
        let Some(first) = first else { continue };
        let line = statement[first].0;
        if words[first].is("DELETE") && at(first + 1).is_some_and(|t| t.is("FROM")) {
            let mut i = first + 2;
            if at(i).is_some_and(|t| t.is("ONLY")) {
                i += 1;
            }
            let filtered = (i..words.len()).any(|j| top(j) && words[j].is("WHERE"));
            if let Some((name, _)) = name_at(&words, i)
                && !filtered
                && !new(&name)
            {
                out.push((line, name));
            }
        } else if words[first].is("TRUNCATE") {
            let mut i = first + 1;
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
                    out.push((line, name));
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

fn last(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_lowercase()
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
    fn a_table_the_same_migration_creates_holds_no_rows_from_before() {
        assert!(
            found("CREATE TABLE IF NOT EXISTS s.t (a int);\nDELETE FROM s.t;\nTRUNCATE t;")
                .is_empty()
        );
        // A statement after a trigger body is read again.
        assert_eq!(
            found("CREATE TRIGGER tr AFTER INSERT ON a BEGIN DELETE FROM b; END;\nDELETE FROM a;"),
            [(2, "a".to_string())]
        );
    }
}

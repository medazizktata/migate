//! Just enough SQL lexing to read what SQLite keeps only in `sqlite_schema.sql`: CHECK constraints, generated column
//! expressions, collations, index expressions and partial-index predicates, trigger timing and events.
//!
//! The lexer understands quotes ('…', "…", `…`, […]), comments and nested parentheses, so a keyword inside a string or a
//! sub-expression is never mistaken for structure.

/// A lexical token. `Group` holds the text inside a balanced pair of parentheses, verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Word(String),
    Quoted(String),
    Str(String),
    Group(String),
    Punct(char),
}

impl Tok {
    /// The token as an identifier: a word or a quoted name, unquoted.
    pub fn ident(&self) -> Option<&str> {
        match self {
            Tok::Word(w) | Tok::Quoted(w) => Some(w),
            _ => None,
        }
    }

    pub fn is_kw(&self, kw: &str) -> bool {
        matches!(self, Tok::Word(w) if w.eq_ignore_ascii_case(kw))
    }
}

pub fn tokens(text: &str) -> Vec<Tok> {
    let c: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '-' && c.get(i + 1) == Some(&'-') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(c.len());
        } else if ch == '(' {
            let end = matching(&c, i);
            out.push(Tok::Group(c[i + 1..end.min(c.len())].iter().collect()));
            i = end + 1;
        } else if matches!(ch, '\'' | '"' | '`' | '[') {
            let close = if ch == '[' { ']' } else { ch };
            let mut j = i + 1;
            let mut s = String::new();
            while j < c.len() {
                if c[j] == close {
                    if close != ']' && c.get(j + 1) == Some(&close) {
                        s.push(close);
                        j += 2;
                        continue;
                    }
                    break;
                }
                s.push(c[j]);
                j += 1;
            }
            out.push(if ch == '\'' {
                Tok::Str(s)
            } else {
                Tok::Quoted(s)
            });
            i = j + 1;
        } else if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            let start = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$') {
                i += 1;
            }
            out.push(Tok::Word(c[start..i].iter().collect()));
        } else {
            out.push(Tok::Punct(ch));
            i += 1;
        }
    }
    out
}

/// The index of the `)` matching the `(` at `open`, skipping quotes; the text's end if unbalanced.
fn matching(c: &[char], open: usize) -> usize {
    let mut depth = 0usize;
    let mut i = open;
    while i < c.len() {
        match c[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            q @ ('\'' | '"' | '`') => {
                i += 1;
                while i < c.len() && c[i] != q {
                    i += 1;
                }
            }
            '[' => {
                while i < c.len() && c[i] != ']' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    c.len()
}

/// Split text on top-level commas (not inside parentheses or quotes), trimming each part.
pub fn split_top_level(text: &str) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '(' => i = matching(&c, i),
            q @ ('\'' | '"' | '`') => {
                i += 1;
                while i < c.len() && c[i] != q {
                    i += 1;
                }
            }
            '[' => {
                while i < c.len() && c[i] != ']' {
                    i += 1;
                }
            }
            ',' => {
                parts.push(c[start..i].iter().collect::<String>().trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last: String = c[start.min(c.len())..]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

/// The text after the first top-level `(` of a statement, up to its matching `)`, and the text after that.
pub fn first_group(text: &str) -> Option<(String, String)> {
    let c: Vec<char> = text.chars().collect();
    let open = c.iter().position(|&ch| ch == '(')?;
    let close = matching(&c, open);
    let inner = c[open + 1..close.min(c.len())].iter().collect();
    let rest = c[(close + 1).min(c.len())..].iter().collect();
    Some((inner, rest))
}

/// Collapse whitespace outside quotes, so the same expression written on several lines compares equal.
pub fn squash(text: &str) -> String {
    let mut out = String::new();
    let mut quote: Option<char> = None;
    let mut space = false;
    for ch in text.trim().chars() {
        match quote {
            Some(q) => {
                out.push(ch);
                if ch == q {
                    quote = None;
                }
            }
            None if ch.is_whitespace() => space = true,
            None => {
                if space && !out.is_empty() {
                    out.push(' ');
                }
                space = false;
                if matches!(ch, '\'' | '"' | '`') {
                    quote = Some(ch);
                }
                out.push(ch);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_top_level_commas_only() {
        let parts = split_top_level("a INT DEFAULT (1, 2), b TEXT CHECK (b <> ','), \"c,d\" INT");
        assert_eq!(
            parts,
            [
                "a INT DEFAULT (1, 2)",
                "b TEXT CHECK (b <> ',')",
                "\"c,d\" INT"
            ]
        );
    }

    #[test]
    fn keywords_inside_strings_and_groups_are_not_structure() {
        let t = tokens("x TEXT DEFAULT 'CHECK (1)' CHECK (x <> ')')");
        assert_eq!(t.iter().filter(|t| t.is_kw("CHECK")).count(), 1);
        assert!(t.contains(&Tok::Group("x <> ')'".into())));
        assert!(t.contains(&Tok::Str("CHECK (1)".into())));
    }

    #[test]
    fn squash_keeps_whitespace_inside_quotes() {
        assert_eq!(squash("  a   =\n 'x  y'  "), "a = 'x  y'");
    }

    #[test]
    fn first_group_returns_body_and_rest() {
        let (inner, rest) = first_group("CREATE INDEX i ON t (a, lower(b)) WHERE a > 0").unwrap();
        assert_eq!(inner, "a, lower(b)");
        assert_eq!(rest.trim(), "WHERE a > 0");
    }
}

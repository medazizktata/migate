//! The schema diff and the breaking-change classifier (`docs/research/architecture.md` §6).
//!
//! [`diff`] compares two snapshots object by object; [`classify`] turns the changes into findings under the MG1xx-MG6xx
//! rules. Both are pure. What a schema diff cannot see is left to the rules that can: a rename shows as a drop plus an
//! add until an explicit marker says otherwise (MG102, MG202), and table rebuilds, index build options, data changes and
//! production's rows need statements, a seed or production itself.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::rules::rule;
use crate::schema::{DataType, Detail, Kind, Object, Snapshot, TypeFamily};
use crate::{Finding, Severity};

/// Every rule [`classify`] can emit.
pub const EMITTED: [&str; 23] = [
    "MG101", "MG201", "MG203", "MG204", "MG205", "MG206", "MG207", "MG208", "MG209", "MG210",
    "MG301", "MG302", "MG303", "MG304", "MG305", "MG402", "MG501", "MG502", "MG503", "MG504",
    "MG505", "MG601", "MG603",
];

/// One object that differs between the base and head snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change<'a> {
    Added(&'a Object),
    Dropped(&'a Object),
    Changed { base: &'a Object, head: &'a Object },
}

/// Every object added, dropped or changed from `base` to `head`, in key order.
///
/// A kind either reader cannot see is skipped: an object missing because the reader is blind to it is not a drop.
pub fn diff<'a>(base: &'a Snapshot, head: &'a Snapshot) -> Vec<Change<'a>> {
    let seen = |o: &&Object| base.sees(o.kind()) && head.sees(o.kind());
    let mut b = base.objects().iter().filter(seen).peekable();
    let mut h = head.objects().iter().filter(seen).peekable();
    let mut out = Vec::new();
    loop {
        match (b.peek(), h.peek()) {
            (None, None) => return out,
            (Some(x), None) => {
                out.push(Change::Dropped(x));
                b.next();
            }
            (None, Some(y)) => {
                out.push(Change::Added(y));
                h.next();
            }
            (Some(x), Some(y)) => match x.key().cmp(&y.key()) {
                Ordering::Less => {
                    out.push(Change::Dropped(x));
                    b.next();
                }
                Ordering::Greater => {
                    out.push(Change::Added(y));
                    h.next();
                }
                Ordering::Equal => {
                    if x != y {
                        out.push(Change::Changed { base: x, head: y });
                    }
                    b.next();
                    h.next();
                }
            },
        }
    }
}

/// The findings for a list of changes. Objects inside a table that was itself added or dropped are not reported again:
/// a new table has no running code to break, and a dropped one is already an error.
pub fn classify(changes: &[Change]) -> Vec<Finding> {
    classify_with(changes, &BTreeSet::new())
}

/// [`classify`], knowing which columns (`namespace.table.column`) hold values after the migration: a new column the
/// migration backfilled is not empty, so a constraint on it can fail on production rows.
pub fn classify_with(changes: &[Change], filled: &BTreeSet<String>) -> Vec<Finding> {
    let whole: BTreeSet<(&str, &str)> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Added(o) | Change::Dropped(o) if o.kind() == Kind::Table => {
                Some((o.namespace.as_str(), o.name.as_str()))
            }
            _ => None,
        })
        .collect();
    // Columns this change adds that start empty (no default, not generated, not identity): every existing row holds
    // NULL there and no old code writes them, so a reference or UNIQUE on them alone cannot fail or break anything.
    let empty_new: BTreeSet<(&str, &str, &str)> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Added(o) => match (&o.detail, &o.parent) {
                (Detail::Column(col), Some(table))
                    if col.default.is_none()
                        && col.generated.is_none()
                        && !col.identity
                        && !filled.contains(&format!("{}.{table}.{}", o.namespace, o.name)) =>
                {
                    Some((o.namespace.as_str(), table.as_str(), o.name.as_str()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    // A column dropped and one added at the same position with the same definition, in one table: perhaps a rename.
    // Both findings stand (a rename breaks the running app as surely as a drop); this only names the pairing, which
    // a schema diff cannot prove (MG202 would need an explicit marker).
    let renames: Vec<(&Object, &Object)> = changes
        .iter()
        .filter_map(|c| match c {
            Change::Dropped(d) => Some(*d),
            _ => None,
        })
        .filter_map(|d| {
            let Detail::Column(was) = &d.detail else {
                return None;
            };
            changes.iter().find_map(|c| match c {
                Change::Added(a)
                    if a.namespace == d.namespace
                        && a.parent == d.parent
                        && matches!(&a.detail, Detail::Column(now) if now == was) =>
                {
                    Some((d, *a))
                }
                _ => None,
            })
        })
        .collect();
    let mut out = Vec::new();
    for change in changes {
        let o = match change {
            Change::Added(o) | Change::Dropped(o) => o,
            Change::Changed { head, .. } => head,
        };
        if o.parent
            .as_deref()
            .is_some_and(|p| whole.contains(&(o.namespace.as_str(), p)))
        {
            continue;
        }
        let start = out.len();
        let mut emit = |id: &'static str, message: String| out.push(finding(id, o, message));
        match change {
            Change::Added(o) => added(o, &mut emit),
            Change::Dropped(o) => dropped(o, &mut emit),
            Change::Changed { base, head } => changed(base, head, &mut emit),
        }
        let hint = match change {
            Change::Dropped(o) => renames.iter().find(|(d, _)| std::ptr::eq(*d, *o)).map(
                |(_, a)| {
                    format!(
                        "; or renamed to {}: the running app still uses the old name either way",
                        a.name
                    )
                },
            ),
            Change::Added(o) => renames
                .iter()
                .find(|(_, a)| std::ptr::eq(*a, *o))
                .map(|(d, _)| format!("; or the renamed {}", d.name)),
            Change::Changed { .. } => None,
        };
        if let Some(hint) = hint {
            for f in &mut out[start..] {
                f.message.push_str(&hint);
            }
        }
        if let (Change::Added(o), Some(table)) = (change, o.parent.as_deref())
            && constrained_columns(o).is_some_and(|cols| {
                !cols.is_empty()
                    && cols
                        .iter()
                        .all(|c| empty_new.contains(&(o.namespace.as_str(), table, c.as_str())))
            })
        {
            for f in &mut out[start..] {
                f.severity = Severity::Note;
                f.message
                    .push_str(" on a column this change adds, empty in every existing row");
            }
        }
    }
    out
}

/// The plain columns a foreign key, UNIQUE or unique index constrains; `None` for anything else (a primary key forces
/// NOT NULL, which existing rows fail; an expression or CHECK reads columns we cannot list).
fn constrained_columns(o: &Object) -> Option<Vec<String>> {
    match &o.detail {
        Detail::ForeignKey(fk) => Some(fk.columns.clone()),
        Detail::Unique { columns } => Some(columns.clone()),
        Detail::Index(i) if i.unique => i
            .parts
            .iter()
            .map(|p| match p {
                crate::schema::IndexPart::Column { name, .. } => Some(name.clone()),
                crate::schema::IndexPart::Expression { .. } => None,
            })
            .collect(),
        _ => None,
    }
}

fn finding(id: &'static str, o: &Object, message: String) -> Finding {
    let severity = rule(id).map_or(Severity::Error, |r| r.severity());
    Finding::new(id, severity, label(o), message)
}

/// `namespace.table.column`, `namespace.table` and so on.
fn label(o: &Object) -> String {
    match &o.parent {
        Some(p) => format!("{}.{p}.{}", o.namespace, o.name),
        None => format!("{}.{}", o.namespace, o.name),
    }
}

fn added(o: &Object, emit: &mut impl FnMut(&'static str, String)) {
    match &o.detail {
        Detail::Column(c) if c.generated.is_some() || c.identity => {
            emit("MG208", "generated or identity column added".into());
        }
        Detail::Column(c) if !c.nullable && c.default.is_none() => {
            emit(
                "MG203",
                format!(
                    "NOT NULL column added without a default ({})",
                    c.data_type.raw
                ),
            );
        }
        Detail::Column(_) => emit("MG210", "column added".into()),
        Detail::PrimaryKey { columns } => emit(
            "MG305",
            format!("primary key added on ({})", columns.join(", ")),
        ),
        Detail::Unique { columns } => {
            emit("MG302", format!("UNIQUE added on ({})", columns.join(", ")))
        }
        Detail::Index(i) if i.unique => emit("MG302", "unique index added".into()),
        Detail::Check { expression, .. } => emit("MG301", format!("CHECK ({expression}) added")),
        Detail::ForeignKey(fk) => emit(
            "MG303",
            format!(
                "foreign key ({}) -> {}.{} added",
                fk.columns.join(", "),
                fk.ref_namespace,
                fk.ref_table
            ),
        ),
        Detail::Trigger { timing, events, .. } => {
            emit(
                "MG503",
                format!("{timing} {} trigger added", events.join("/")),
            );
        }
        // shortcut: an index built without the online option (MG401) needs the migration's statements, not the result.
        _ => {}
    }
}

fn dropped(o: &Object, emit: &mut impl FnMut(&'static str, String)) {
    let id = match &o.detail {
        Detail::Table { .. } => "MG101",
        Detail::Column(_) => "MG201",
        Detail::PrimaryKey { .. }
        | Detail::Unique { .. }
        | Detail::Check { .. }
        | Detail::ForeignKey(_) => "MG304",
        Detail::Index(_) => "MG402",
        Detail::View { .. } => "MG501",
        Detail::Trigger { .. } => "MG504",
        Detail::Routine { .. } => "MG505",
        Detail::Enum { .. } => "MG601",
        Detail::Sequence { .. } => "MG603",
        Detail::Namespace {} => return, // its contents are reported one by one
    };
    emit(id, format!("{} dropped", noun(o.kind())));
}

fn noun(kind: Kind) -> &'static str {
    match kind {
        Kind::Namespace => "namespace",
        Kind::Table => "table",
        Kind::Column => "column",
        Kind::PrimaryKey => "primary key",
        Kind::Unique => "UNIQUE constraint",
        Kind::Check => "CHECK constraint",
        Kind::ForeignKey => "foreign key",
        Kind::Index => "index",
        Kind::View => "view",
        Kind::Trigger => "trigger",
        Kind::Routine => "routine",
        Kind::Enum => "enum",
        Kind::Sequence => "sequence",
    }
}

fn changed(base: &Object, head: &Object, emit: &mut impl FnMut(&'static str, String)) {
    match (&base.detail, &head.detail) {
        (Detail::Column(a), Detail::Column(b)) => {
            if a.nullable && !b.nullable {
                emit("MG204", "column became NOT NULL".into());
            }
            let (ta, tb) = (&a.data_type, &b.data_type);
            if narrows(ta, tb) {
                emit(
                    "MG205",
                    format!("type narrowed from {} to {}", ta.raw, tb.raw),
                );
            } else if !same_type(ta, tb) {
                emit(
                    "MG206",
                    format!("type changed from {} to {}", ta.raw, tb.raw),
                );
            }
            if a.collation != b.collation {
                emit(
                    "MG206",
                    format!(
                        "collation changed from {:?} to {:?}",
                        a.collation, b.collation
                    ),
                );
            }
            match (&a.default, &b.default) {
                (Some(d), None) => emit("MG207", format!("default {d} dropped")),
                (Some(x), Some(y)) if x != y => {
                    emit("MG209", format!("default changed from {x} to {y}"))
                }
                _ => {}
            }
            if a.generated != b.generated || a.identity != b.identity {
                emit("MG208", "generated expression or identity changed".into());
            }
        }
        (Detail::PrimaryKey { columns }, _) => emit(
            "MG305",
            format!("primary key changed from ({})", columns.join(", ")),
        ),
        (Detail::Unique { .. }, _) => emit("MG302", "UNIQUE constraint changed".into()),
        (Detail::Check { expression: x, .. }, Detail::Check { expression: y, .. }) => {
            emit(
                "MG301",
                if x == y {
                    "CHECK now validated".into()
                } else {
                    format!("CHECK changed to ({y})")
                },
            );
        }
        (Detail::ForeignKey(_), _) => emit("MG303", "foreign key changed".into()),
        (Detail::Index(_), Detail::Index(i)) if i.unique => {
            emit("MG302", "unique index changed".into())
        }
        (Detail::Index(_), _) => emit("MG402", "index changed".into()),
        (Detail::View { .. }, _) => emit("MG502", "view definition changed".into()),
        (Detail::Trigger { .. }, _) => emit("MG504", "trigger changed".into()),
        (Detail::Routine { arguments: x, .. }, Detail::Routine { arguments: y, .. }) if x != y => {
            emit(
                "MG505",
                format!("routine arguments changed from ({x}) to ({y})"),
            );
        }
        (Detail::Enum { values: x }, Detail::Enum { values: y }) => {
            let gone: Vec<&str> = x
                .iter()
                .filter(|v| !y.contains(v))
                .map(String::as_str)
                .collect();
            if !gone.is_empty() {
                emit("MG601", format!("enum values removed: {}", gone.join(", ")));
            }
        }
        // shortcut: table options (SQLite STRICT, WITHOUT ROWID), routine bodies and sequence types are compared but not
        // classified; the mixed-version test is what judges them.
        _ => {}
    }
}

/// Same family and size, and a declared name that differs at most in case or spacing.
fn same_type(a: &DataType, b: &DataType) -> bool {
    let squash = |s: &str| {
        s.split_whitespace()
            .collect::<String>()
            .to_ascii_uppercase()
    };
    (a.family, a.length, a.precision, a.scale) == (b.family, b.length, b.precision, b.scale)
        && squash(&a.raw) == squash(&b.raw)
}

/// Whether some value of type `a` may not fit in type `b`.
fn narrows(a: &DataType, b: &DataType) -> bool {
    use TypeFamily::*;
    // A size limit added or lowered; `None` is unbounded.
    let smaller = |x: Option<u32>, y: Option<u32>| y.is_some_and(|y| x.is_none_or(|x| y < x));
    if a.family != b.family {
        let widens = matches!(
            (a.family, b.family),
            (_, Text) | (Integer, Decimal) | (Date, Timestamp)
        );
        return !widens || smaller(None, b.length) || smaller(None, b.precision);
    }
    let digits = |t: &DataType| t.precision.map(|p| p.saturating_sub(t.scale.unwrap_or(0)));
    smaller(a.length, b.length)
        || smaller(digits(a), digits(b))
        || smaller(
            a.scale.or(a.precision.map(|_| 0)),
            b.scale.or(b.precision.map(|_| 0)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, ForeignKey, Generated, Index, IndexPart};

    const ALL_KINDS: [Kind; 13] = [
        Kind::Namespace,
        Kind::Table,
        Kind::Column,
        Kind::PrimaryKey,
        Kind::Unique,
        Kind::Check,
        Kind::ForeignKey,
        Kind::Index,
        Kind::View,
        Kind::Trigger,
        Kind::Routine,
        Kind::Enum,
        Kind::Sequence,
    ];

    fn ty(
        raw: &str,
        family: TypeFamily,
        length: Option<u32>,
        precision: Option<u32>,
        scale: Option<u32>,
    ) -> DataType {
        DataType {
            raw: raw.into(),
            family,
            length,
            precision,
            scale,
        }
    }

    fn text() -> DataType {
        ty("TEXT", TypeFamily::Text, None, None, None)
    }

    fn column(
        name: &str,
        ordinal: u32,
        data_type: DataType,
        nullable: bool,
        default: Option<&str>,
    ) -> Object {
        Object {
            namespace: "main".into(),
            name: name.into(),
            parent: Some("t".into()),
            detail: Detail::Column(Column {
                ordinal,
                data_type,
                nullable,
                default: default.map(Into::into),
                generated: None,
                identity: false,
                collation: None,
            }),
        }
    }

    fn on_t(name: &str, detail: Detail) -> Object {
        Object {
            namespace: "main".into(),
            name: name.into(),
            parent: Some("t".into()),
            detail,
        }
    }

    fn top(name: &str, detail: Detail) -> Object {
        Object {
            namespace: "main".into(),
            name: name.into(),
            parent: None,
            detail,
        }
    }

    /// The fixture every case starts from: table `t (id, a TEXT)`, plus whatever else the case needs on both sides.
    fn base_objects() -> Vec<Object> {
        vec![
            top("main", Detail::Namespace {}),
            top(
                "t",
                Detail::Table {
                    strict: false,
                    without_rowid: false,
                },
            ),
            column(
                "id",
                0,
                ty("INTEGER", TypeFamily::Integer, None, None, None),
                false,
                None,
            ),
            column("a", 1, text(), true, Some("'x'")),
        ]
    }

    fn snap(objects: Vec<Object>) -> Snapshot {
        Snapshot::new("test", None, ALL_KINDS.into_iter().collect(), objects).unwrap()
    }

    fn rules_between(base: Vec<Object>, head: Vec<Object>) -> Vec<&'static str> {
        let (b, h) = (snap(base), snap(head));
        classify(&diff(&b, &h))
            .into_iter()
            .map(|f| f.rule)
            .collect()
    }

    /// Replace the object with `o`'s key, or add it.
    fn with(mut objects: Vec<Object>, o: Object) -> Vec<Object> {
        objects.retain(|x| x.key() != o.key());
        objects.push(o);
        objects
    }

    fn without(mut objects: Vec<Object>, kind: Kind, name: &str) -> Vec<Object> {
        objects.retain(|x| !(x.kind() == kind && x.name == name));
        objects
    }

    fn index(unique: bool, cols: &[&str]) -> Detail {
        Detail::Index(Index {
            unique,
            parts: cols
                .iter()
                .map(|c| IndexPart::Column {
                    name: (*c).into(),
                    descending: false,
                })
                .collect(),
            predicate: None,
            method: None,
        })
    }

    fn fk(on_delete: &str) -> Detail {
        Detail::ForeignKey(ForeignKey {
            columns: vec!["a".into()],
            ref_namespace: "main".into(),
            ref_table: "p".into(),
            ref_columns: vec!["id".into()],
            on_delete: on_delete.into(),
            on_update: "NO ACTION".into(),
            deferrable: false,
            initially_deferred: false,
            validated: true,
        })
    }

    fn trigger(definition: &str) -> Detail {
        Detail::Trigger {
            timing: "AFTER".into(),
            events: vec!["INSERT".into()],
            definition: definition.into(),
        }
    }

    fn generated(expression: &str) -> Object {
        let mut c = column("g", 2, text(), true, None);
        if let Detail::Column(col) = &mut c.detail {
            col.generated = Some(Generated {
                expression: expression.into(),
                stored: false,
            });
        }
        c
    }

    struct Case {
        rule: &'static str,
        base: Vec<Object>,
        bad: Vec<Object>,
        good: Vec<Object>,
    }

    /// Per rule: a base, a head that must raise the rule, and a nearby safe head that must not.
    fn cases() -> Vec<Case> {
        let b = base_objects;
        let with_extra = |extra: Vec<Object>| {
            let mut v = b();
            v.extend(extra);
            v
        };
        let dec = |p, s| {
            ty(
                &format!("DECIMAL({p},{s})"),
                TypeFamily::Decimal,
                None,
                Some(p),
                Some(s),
            )
        };
        let num_col = |t: DataType| column("n", 2, t, true, None);
        let routine = |args: &str, body: &str| {
            top(
                "f",
                Detail::Routine {
                    arguments: args.into(),
                    definition_sha256: body.into(),
                },
            )
        };
        let en = |v: &[&str]| {
            top(
                "mood",
                Detail::Enum {
                    values: v.iter().map(|s| (*s).into()).collect(),
                },
            )
        };
        let seq = top(
            "s",
            Detail::Sequence {
                data_type: "bigint".into(),
                owned_by: None,
            },
        );
        let view = |d: &str| {
            top(
                "v",
                Detail::View {
                    definition: d.into(),
                    materialized: false,
                },
            )
        };
        let pk = |c: &[&str]| {
            on_t(
                "t_pkey",
                Detail::PrimaryKey {
                    columns: c.iter().map(|s| (*s).into()).collect(),
                },
            )
        };
        let unique = |c: &[&str]| {
            on_t(
                "t_a_key",
                Detail::Unique {
                    columns: c.iter().map(|s| (*s).into()).collect(),
                },
            )
        };
        let collated = |name: &str| {
            let mut c = column("a", 1, text(), true, Some("'x'"));
            if let Detail::Column(col) = &mut c.detail {
                col.collation = Some(name.into());
            }
            c
        };
        let check = |e: &str| {
            on_t(
                "t_check",
                Detail::Check {
                    expression: e.into(),
                    validated: true,
                },
            )
        };
        vec![
            Case {
                rule: "MG101",
                base: b(),
                bad: without(
                    without(without(b(), Kind::Table, "t"), Kind::Column, "id"),
                    Kind::Column,
                    "a",
                ),
                good: with(
                    b(),
                    top(
                        "u",
                        Detail::Table {
                            strict: false,
                            without_rowid: false,
                        },
                    ),
                ),
            },
            Case {
                rule: "MG201",
                base: b(),
                bad: without(b(), Kind::Column, "a"),
                good: with(b(), column("c", 2, text(), true, None)),
            },
            Case {
                rule: "MG203",
                base: b(),
                bad: with(b(), column("c", 2, text(), false, None)),
                good: with(b(), column("c", 2, text(), false, Some("''"))),
            },
            Case {
                rule: "MG204",
                base: b(),
                bad: with(b(), column("a", 1, text(), false, Some("'x'"))),
                good: with(
                    b(),
                    column(
                        "id",
                        0,
                        ty("INTEGER", TypeFamily::Integer, None, None, None),
                        false,
                        None,
                    ),
                ),
            },
            Case {
                rule: "MG205",
                base: with_extra(vec![num_col(dec(10, 2))]),
                bad: with_extra(vec![num_col(dec(8, 2))]),
                good: with_extra(vec![num_col(dec(12, 2))]),
            },
            Case {
                rule: "MG206",
                base: b(),
                bad: with(
                    b(),
                    column(
                        "a",
                        1,
                        ty("CLOB", TypeFamily::Text, None, None, None),
                        true,
                        Some("'x'"),
                    ),
                ),
                good: with(
                    b(),
                    column(
                        "a",
                        1,
                        ty("text", TypeFamily::Text, None, None, None),
                        true,
                        Some("'x'"),
                    ),
                ),
            },
            Case {
                rule: "MG207",
                base: b(),
                bad: with(b(), column("a", 1, text(), true, None)),
                good: b(),
            },
            Case {
                rule: "MG208",
                base: b(),
                bad: with(b(), generated("upper(a)")),
                good: with(b(), column("g", 2, text(), true, None)),
            },
            Case {
                rule: "MG209",
                base: b(),
                bad: with(b(), column("a", 1, text(), true, Some("'y'"))),
                good: b(),
            },
            Case {
                rule: "MG210",
                base: b(),
                bad: with(b(), column("c", 2, text(), true, None)),
                good: b(),
            },
            Case {
                rule: "MG301",
                base: b(),
                bad: with(b(), check("a <> ''")),
                good: b(),
            },
            Case {
                rule: "MG302",
                base: b(),
                bad: with(
                    b(),
                    on_t(
                        "t_a_key",
                        Detail::Unique {
                            columns: vec!["a".into()],
                        },
                    ),
                ),
                good: with(b(), on_t("t_a", index(false, &["a"]))),
            },
            Case {
                rule: "MG303",
                base: b(),
                bad: with(b(), on_t("t_a_fkey", fk("CASCADE"))),
                good: b(),
            },
            Case {
                rule: "MG304",
                base: with_extra(vec![check("a <> ''")]),
                bad: b(),
                good: with_extra(vec![check("a <> ''")]),
            },
            Case {
                rule: "MG305",
                base: b(),
                bad: with(
                    b(),
                    on_t(
                        "t_pkey",
                        Detail::PrimaryKey {
                            columns: vec!["id".into()],
                        },
                    ),
                ),
                good: b(),
            },
            Case {
                rule: "MG402",
                base: with_extra(vec![on_t("t_a", index(false, &["a"]))]),
                bad: b(),
                good: with_extra(vec![on_t("t_a", index(false, &["a"]))]),
            },
            Case {
                rule: "MG501",
                base: with_extra(vec![view("SELECT a FROM t")]),
                bad: b(),
                good: with_extra(vec![view("SELECT a FROM t")]),
            },
            Case {
                rule: "MG502",
                base: with_extra(vec![view("SELECT a FROM t")]),
                bad: with_extra(vec![view("SELECT id FROM t")]),
                good: with_extra(vec![view("SELECT a FROM t")]),
            },
            Case {
                rule: "MG503",
                base: b(),
                bad: with(b(), on_t("t_audit", trigger("x"))),
                good: b(),
            },
            Case {
                rule: "MG504",
                base: with_extra(vec![on_t("t_audit", trigger("x"))]),
                bad: with_extra(vec![on_t("t_audit", trigger("y"))]),
                good: with_extra(vec![on_t("t_audit", trigger("x"))]),
            },
            Case {
                rule: "MG505",
                base: with_extra(vec![routine("integer", "h1")]),
                bad: with_extra(vec![routine("integer, text", "h1")]),
                good: with_extra(vec![routine("integer", "h2")]),
            },
            Case {
                rule: "MG601",
                base: with_extra(vec![en(&["sad", "ok"])]),
                bad: with_extra(vec![en(&["sad", "fine"])]),
                good: with_extra(vec![en(&["sad", "ok", "happy"])]),
            },
            Case {
                rule: "MG603",
                base: with_extra(vec![seq.clone()]),
                bad: b(),
                good: with_extra(vec![seq]),
            },
            // The other branches of rules above: changed rather than added, dropped rather than changed.
            Case {
                rule: "MG302",
                base: b(),
                bad: with(b(), on_t("t_a_u", index(true, &["a"]))),
                good: with(b(), on_t("t_a_u", index(false, &["a"]))),
            },
            Case {
                rule: "MG504",
                base: with_extra(vec![on_t("t_audit", trigger("x"))]),
                bad: b(),
                good: with_extra(vec![on_t("t_audit", trigger("x"))]),
            },
            Case {
                rule: "MG505",
                base: with_extra(vec![routine("integer", "h1")]),
                bad: b(),
                good: with_extra(vec![routine("integer", "h1")]),
            },
            Case {
                rule: "MG601",
                base: with_extra(vec![en(&["sad"])]),
                bad: b(),
                good: with_extra(vec![en(&["sad"])]),
            },
            Case {
                rule: "MG206",
                base: b(),
                bad: with(b(), collated("NOCASE")),
                good: b(),
            },
            Case {
                rule: "MG208",
                base: with(b(), generated("upper(a)")),
                bad: with(b(), generated("lower(a)")),
                good: with(b(), generated("upper(a)")),
            },
            Case {
                rule: "MG305",
                base: with(b(), pk(&["id"])),
                bad: with(b(), pk(&["id", "a"])),
                good: with(b(), pk(&["id"])),
            },
            Case {
                rule: "MG302",
                base: with(b(), unique(&["a"])),
                bad: with(b(), unique(&["a", "id"])),
                good: with(b(), unique(&["a"])),
            },
            Case {
                rule: "MG301",
                base: with(b(), check("a <> ''")),
                bad: with(b(), check("length(a) > 1")),
                good: with(b(), check("a <> ''")),
            },
            Case {
                rule: "MG303",
                base: with(b(), on_t("t_a_fkey", fk("CASCADE"))),
                bad: with(b(), on_t("t_a_fkey", fk("SET NULL"))),
                good: with(b(), on_t("t_a_fkey", fk("CASCADE"))),
            },
            Case {
                rule: "MG302",
                base: with(b(), on_t("t_a_u", index(true, &["a"]))),
                bad: with(b(), on_t("t_a_u", index(true, &["a", "id"]))),
                good: with(b(), on_t("t_a_u", index(false, &["a", "id"]))),
            },
            Case {
                rule: "MG402",
                base: with(b(), on_t("t_a", index(false, &["a"]))),
                bad: with(b(), on_t("t_a", index(false, &["a", "id"]))),
                good: with(b(), on_t("t_a", index(false, &["a"]))),
            },
        ]
    }

    #[test]
    fn every_rule_fails_its_bad_fixture_and_passes_its_good_one() {
        let cases = cases();
        let covered: BTreeSet<&str> = cases.iter().map(|c| c.rule).collect();
        assert_eq!(
            covered,
            EMITTED.into_iter().collect(),
            "each emitted rule needs a case"
        );
        for c in cases {
            let bad = rules_between(c.base.clone(), c.bad);
            assert!(
                bad.contains(&c.rule),
                "{}: bad fixture raised {bad:?}",
                c.rule
            );
            let good = rules_between(c.base, c.good);
            assert!(
                !good.contains(&c.rule),
                "{}: good fixture raised {good:?}",
                c.rule
            );
        }
    }

    #[test]
    fn identical_snapshots_have_no_changes() {
        assert!(diff(&snap(base_objects()), &snap(base_objects())).is_empty());
    }

    #[test]
    fn contents_of_a_new_or_dropped_table_are_not_reported_again() {
        let mut head = base_objects();
        head.extend([
            top(
                "u",
                Detail::Table {
                    strict: false,
                    without_rowid: false,
                },
            ),
            Object {
                parent: Some("u".into()),
                ..column("x", 0, text(), false, None)
            },
        ]);
        assert_eq!(rules_between(base_objects(), head), Vec::<&str>::new());
        let dropped = rules_between(base_objects(), vec![top("main", Detail::Namespace {})]);
        assert_eq!(dropped, ["MG101"]);
    }

    #[test]
    fn a_kind_one_reader_cannot_see_is_not_a_drop() {
        let mut base = base_objects();
        base.push(top(
            "f",
            Detail::Routine {
                arguments: String::new(),
                definition_sha256: String::new(),
            },
        ));
        let b = snap(base);
        let h = Snapshot::new(
            "test",
            None,
            [Kind::Namespace, Kind::Table, Kind::Column]
                .into_iter()
                .collect(),
            base_objects(),
        )
        .unwrap();
        assert!(diff(&b, &h).is_empty());
    }

    #[test]
    fn narrowing_follows_family_and_size() {
        use TypeFamily::*;
        let t = |f, l| ty("x", f, l, None, None);
        assert!(narrows(&t(Text, None), &t(Text, Some(10))));
        assert!(narrows(&t(Text, Some(20)), &t(Text, Some(10))));
        assert!(!narrows(&t(Text, Some(10)), &t(Text, Some(20))));
        assert!(!narrows(&t(Text, Some(10)), &t(Text, None)));
        assert!(narrows(&t(Text, None), &t(Integer, None)));
        assert!(!narrows(&t(Integer, None), &t(Text, None)));
        assert!(narrows(&t(Integer, None), &t(Text, Some(5))));
        assert!(!narrows(&t(Integer, None), &t(Decimal, None)));
        assert!(narrows(&t(Timestamp, None), &t(Date, None)));
        let d = |p, s| ty("x", Decimal, None, Some(p), Some(s));
        assert!(narrows(&d(10, 2), &d(10, 4)), "fewer integer digits");
        assert!(narrows(&d(10, 4), &d(10, 2)), "fewer decimals");
        assert!(!narrows(&d(10, 2), &d(12, 2)));
    }

    #[test]
    fn a_constraint_on_a_column_the_change_adds_empty_is_a_note() {
        let base = with(base_objects(), on_t("t_a", index(false, &["a"])));
        let fk_on = |col: &str| {
            on_t(
                &format!("t_{col}_fkey"),
                Detail::ForeignKey(ForeignKey {
                    columns: vec![col.into()],
                    ..match fk("CASCADE") {
                        Detail::ForeignKey(f) => f,
                        _ => unreachable!(),
                    }
                }),
            )
        };
        let found = |head: Vec<Object>| {
            let (b, h) = (snap(base.clone()), snap(head));
            classify(&diff(&b, &h))
                .into_iter()
                .map(|f| (f.rule, f.severity))
                .collect::<Vec<_>>()
        };
        // A new, empty column: no existing row holds a value, no old code writes it.
        let mut head = with(base.clone(), column("owner", 2, text(), true, None));
        head.push(fk_on("owner"));
        head.push(on_t(
            "t_owner_key",
            Detail::Unique {
                columns: vec!["owner".into()],
            },
        ));
        head.push(on_t("t_owner_idx", index(true, &["owner"])));
        assert_eq!(
            found(head),
            [
                ("MG210", Severity::Note),
                ("MG302", Severity::Note),
                ("MG303", Severity::Note),
                ("MG302", Severity::Note)
            ]
        );
        // A new column with a default fills every existing row with one value: UNIQUE or the reference can fail.
        let mut head = with(base.clone(), column("owner", 2, text(), true, Some("'x'")));
        head.push(fk_on("owner"));
        assert_eq!(
            found(head),
            [("MG210", Severity::Note), ("MG303", Severity::Error)]
        );
        // An existing column stays an error.
        assert_eq!(
            found(with(base.clone(), fk_on("a"))),
            [("MG303", Severity::Error)]
        );
    }

    #[test]
    fn a_column_change_raises_only_its_own_rule() {
        let mut collated = column("a", 1, text(), true, Some("'x'"));
        if let Detail::Column(c) = &mut collated.detail {
            c.collation = Some("NOCASE".into());
        }
        assert_eq!(
            rules_between(base_objects(), with(base_objects(), collated)),
            ["MG206"]
        );
    }

    #[test]
    fn an_object_named_like_a_table_does_not_hide_that_tables_changes() {
        // MySQL names indexes per table, so an index may share another table's name.
        let mut base = base_objects();
        base.extend([
            top(
                "u",
                Detail::Table {
                    strict: false,
                    without_rowid: false,
                },
            ),
            Object {
                parent: Some("u".into()),
                ..column("x", 0, text(), true, None)
            },
        ]);
        let mut head = without(base.clone(), Kind::Column, "x");
        head.push(on_t("u", index(false, &["a"])));
        assert_eq!(rules_between(base, head), ["MG201"]);
    }

    #[test]
    fn messages_name_what_changed() {
        let b = snap(base_objects());
        let dropped = classify(&diff(&b, &snap(without(base_objects(), Kind::Column, "a"))));
        assert_eq!(dropped[0].message, "column dropped");
        let unvalidated = on_t(
            "t_check",
            Detail::Check {
                expression: "a <> ''".into(),
                validated: false,
            },
        );
        let validated = on_t(
            "t_check",
            Detail::Check {
                expression: "a <> ''".into(),
                validated: true,
            },
        );
        let f = classify(&diff(
            &snap(with(base_objects(), unvalidated)),
            &snap(with(base_objects(), validated)),
        ));
        assert_eq!(f[0].message, "CHECK now validated");
    }

    #[test]
    fn a_drop_and_an_identical_add_in_place_say_they_may_be_a_rename() {
        let b = snap(base_objects());
        let renamed = with(
            without(base_objects(), Kind::Column, "a"),
            column("b", 1, text(), true, Some("'x'")),
        );
        let f = classify(&diff(&b, &snap(renamed)));
        let by_rule = |r: &str| f.iter().find(|f| f.rule == r).map(|f| f.message.clone());
        assert_eq!(
            by_rule("MG201").as_deref(),
            Some(
                "column dropped; or renamed to b: the running app still uses the old name either way"
            )
        );
        assert!(
            f.iter()
                .filter(|f| f.object == "main.t.b")
                .all(|f| f.message.ends_with("; or the renamed a")),
            "{f:?}"
        );
        let retyped = with(
            without(base_objects(), Kind::Column, "a"),
            column(
                "b",
                1,
                ty("INTEGER", TypeFamily::Integer, None, None, None),
                true,
                None,
            ),
        );
        let f = classify(&diff(&b, &snap(retyped)));
        assert!(f.iter().all(|f| !f.message.contains("renamed")), "{f:?}");
    }

    #[test]
    fn severities_follow_the_axes() {
        let b = snap(base_objects());
        let h = snap(without(base_objects(), Kind::Column, "a"));
        let f = &classify(&diff(&b, &h))[0];
        assert_eq!(
            (f.rule, f.severity, f.object.as_str()),
            ("MG201", Severity::Error, "main.t.a")
        );
        let h = snap(with(base_objects(), column("c", 2, text(), true, None)));
        assert_eq!(classify(&diff(&b, &h))[0].severity, Severity::Note);
    }
}

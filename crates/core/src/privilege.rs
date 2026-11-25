//! Privileges the migrations take away (MG211): the version still serving traffic connects as some role, and a role
//! that loses SELECT or INSERT breaks it as surely as a dropped column.

use std::collections::BTreeSet;

use crate::{Finding, Severity};

/// Who may do what on an object, after the engine's defaults are applied (an owner's implicit rights, PUBLIC's
/// EXECUTE on a routine). `grantee` is a role name, or `PUBLIC` for every role.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Privilege {
    /// `table`, `column`, `routine` or `schema`.
    pub kind: String,
    /// `namespace.table`, `namespace.table.column`, `namespace.routine(arguments)` or `namespace`.
    pub object: String,
    pub grantee: String,
    /// `SELECT`, `INSERT`, `EXECUTE` and the like.
    pub privilege: String,
}

pub const PUBLIC: &str = "PUBLIC";

/// One finding per privilege held before the migrations and not after, on an object that still exists (`columns`:
/// every `namespace.table.column` after; a dropped object is reported by the schema diff). A role keeps a privilege
/// PUBLIC still holds. PUBLIC losing one that named roles hold is a warning: narrowing it to the app's own role is
/// common, and only the app knows which role it connects as.
// shortcut: role membership is not followed (a role that inherits from a holder); read pg_auth_members if a project
// relies on it.
pub fn lost(
    before: &BTreeSet<Privilege>,
    after: &BTreeSet<Privilege>,
    columns: &BTreeSet<String>,
) -> Vec<Finding> {
    let exists = |p: &Privilege| {
        if p.kind == "column" {
            columns.contains(&p.object)
        } else {
            after
                .iter()
                .any(|a| a.kind == p.kind && a.object == p.object)
        }
    };
    let holders = |p: &Privilege| -> Vec<&str> {
        after
            .iter()
            .filter(|a| a.kind == p.kind && a.object == p.object && a.privilege == p.privilege)
            .map(|a| a.grantee.as_str())
            .collect()
    };
    before
        .difference(after)
        .filter(|p| exists(p))
        .filter_map(|p| {
            let now = holders(p);
            if now.contains(&PUBLIC) {
                return None;
            }
            let (severity, who) = if p.grantee == PUBLIC {
                if now.is_empty() {
                    (Severity::Error, "every role (PUBLIC)".to_string())
                } else {
                    (
                        Severity::Warning,
                        format!("every role but {} (PUBLIC)", now.join(", ")),
                    )
                }
            } else {
                (Severity::Error, p.grantee.clone())
            };
            Some(Finding::new(
                "MG211",
                severity,
                p.object.clone(),
                format!(
                    "{who} loses {} on this {}: the base version fails there if it connects as such a role",
                    p.privilege, p.kind
                ),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(kind: &str, object: &str, grantee: &str, privilege: &str) -> Privilege {
        Privilege {
            kind: kind.into(),
            object: object.into(),
            grantee: grantee.into(),
            privilege: privilege.into(),
        }
    }

    fn set(p: &[Privilege]) -> BTreeSet<Privilege> {
        p.iter().cloned().collect()
    }

    fn found(f: &[Finding]) -> Vec<(Severity, &str, &str)> {
        f.iter()
            .map(|f| (f.severity, f.object.as_str(), f.message.as_str()))
            .collect()
    }

    #[test]
    fn a_role_losing_a_privilege_is_an_error() {
        let owner = p("table", "public.posts", "owner", "INSERT");
        let insert = p("table", "public.posts", "authenticated", "INSERT");
        let select = p("table", "public.posts", "authenticated", "SELECT");
        let f = lost(
            &set(&[owner.clone(), insert, select.clone()]),
            &set(&[owner, select]),
            &BTreeSet::new(),
        );
        assert_eq!(
            found(&f),
            [(
                Severity::Error,
                "public.posts",
                "authenticated loses INSERT on this table: the base version fails there if it connects as such a role"
            )]
        );
    }

    #[test]
    fn nothing_is_lost_when_privileges_are_only_added() {
        let owner = p("table", "public.posts", "owner", "SELECT");
        let added = p("table", "public.posts", "anon", "SELECT");
        assert!(
            lost(
                &set(std::slice::from_ref(&owner)),
                &set(&[owner, added]),
                &BTreeSet::new()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_dropped_object_and_a_privilege_public_keeps_are_not_reported() {
        let gone = p("table", "public.old", "app", "SELECT");
        let col = p("column", "public.posts.body", "app", "UPDATE");
        let via_public = p("table", "public.posts", "app", "SELECT");
        let public = p("table", "public.posts", PUBLIC, "SELECT");
        let before = set(&[gone, col.clone(), via_public, public.clone()]);
        let after = set(&[public]);
        assert!(lost(&before, &after, &BTreeSet::new()).is_empty());
        // The column is still there: its lost grant is reported.
        let columns = set_of(&["public.posts.body"]);
        assert_eq!(lost(&set(&[col]), &BTreeSet::new(), &columns).len(), 1);
    }

    fn set_of(s: &[&str]) -> BTreeSet<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn public_narrowed_to_named_roles_is_a_warning_and_revoked_outright_an_error() {
        let owner = p("routine", "public.f()", "owner", "EXECUTE");
        let public = p("routine", "public.f()", PUBLIC, "EXECUTE");
        let auth = p("routine", "public.f()", "authenticated", "EXECUTE");
        let narrowed = lost(
            &set(&[owner.clone(), public.clone()]),
            &set(&[owner.clone(), auth]),
            &BTreeSet::new(),
        );
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].severity, Severity::Warning);
        assert!(
            narrowed[0]
                .message
                .contains("every role but authenticated, owner (PUBLIC)"),
            "{}",
            narrowed[0].message
        );
        // Only the owner keeps it: every other role lost it.
        let only_owner = p("schema", "public", "owner", "USAGE");
        let public_usage = p("schema", "public", PUBLIC, "USAGE");
        let revoked = lost(
            &set(&[only_owner.clone(), public_usage]),
            &set(&[only_owner]),
            &BTreeSet::new(),
        );
        assert_eq!(revoked[0].severity, Severity::Warning);
        let alone = lost(
            &set(&[
                p("schema", "s", PUBLIC, "USAGE"),
                p("schema", "s", "x", "CREATE"),
            ]),
            &set(&[p("schema", "s", "x", "CREATE")]),
            &BTreeSet::new(),
        );
        assert_eq!(alone[0].severity, Severity::Error);
    }
}

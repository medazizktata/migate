# Releasing

The repository is private for now, and a private repository on GitHub's free plan gets about 2,000 Linux runner
minutes a month. CI is arranged so that pushing costs almost nothing and the full suite runs once per release.

## What runs when

| Trigger | Workflow | Runs | Runner minutes (measured 2026-11-20) |
|---|---|---|---|
| every push, every pull request | `guard.yml` | nothing sensitive in the history | about 1 |
| a tag `v*` | `release.yml` | the full suite (`ci.yml`), a Linux binary, the GitHub release | about 13, build included |
| by hand (Actions tab, or `gh workflow run`) | `ci.yml` | the full suite: all checks, minimum Rust, PostgreSQL 15-18 | about 13 |
| by hand | `release.yml` | a dry run: the suite and the build, no release | about 13 |
| the 1st of each month, or by hand | `mutants.yml` | mutation testing of `migate-core`, and of the SQL scanner | about 19, plus about 7 (the scanner, measured locally) |

Every commit already runs the checks locally (`.githooks/pre-commit`), and every push runs the sensitive-content
check locally (`.githooks/pre-push`), so CI between releases adds Linux and the PostgreSQL versions, not first
detection.

## When a release is worth making

A release is for someone to use, not to exercise the pipeline. Cut one when all of these hold:

- the work since the last release was tried on a real project (a dogfood run, not only the test suite; every run is in `end-to-end-testing/`), and nothing
  it found that blocks a team is left open;
- no known false alarm or missed break is left open without a task and a decision;
- there is a binary, or an install path, for the people who will use it.

The first release, `0.1.0`, waits on that (TASK-69).

## The cycle

1. Work on `main`, committing as usual. Push whenever a backup is wanted: it costs the one-minute guard.
2. When a batch of work is done (a milestone task, or every week or two), prepare the release:

   ```
   just release 0.2.0
   ```

   It refuses unless the tree is clean, on `main`, and the version is new and newer. It runs every check, sets the
   version in `Cargo.toml` and `Cargo.lock`, adds a `CHANGELOG.md` section listing the commit subjects since the last
   release, commits `Release v0.2.0` and makes the annotated tag `v0.2.0`. Nothing is pushed.
3. Read the new `CHANGELOG.md` section; edit it (and amend the commit, moving the tag) if it needs words.
4. Publish:

   ```
   just publish
   ```

   The pre-push hook checks the commits; the tag starts `release.yml`, which runs the full suite, builds
   `migate-v0.2.0-x86_64-unknown-linux-gnu.tar.gz` with its `.sha256`, and publishes the GitHub release with the
   changelog section as its notes. The release job alone can write to the repository, and it runs no build.

## Versions

`0.y.z` until the first public release. While at 0: the minor number for new checks, rules or configuration, the
patch number for fixes only.

## Not yet

Binaries for macOS and for Linux on ARM, and the npm, pip, Homebrew and GitHub Action packages, are TASK-25. macOS
runners cost ten times the Linux minutes on a private repository, so they wait for the repository to go public or
for a release that needs them.

# The CI runners

CI runs on GitHub's hosted runners first. When a run of `ci`, `guard` or `mutants` fails, including when GitHub never
starts its jobs (as when the account's billing blocks them), `fallback.yml` runs the same workflow again, on the same
commit, on a self-hosted Linux runner labelled `migate`: a home machine whose disk and CPUs other projects' runners
share. A real failure is then seen on both, which tells it apart from a problem on GitHub's side. The release build
(`release.yml`'s own jobs) stays on GitHub's runners. The home machine sleeps at night; a fallback queued then waits,
and GitHub keeps it queued for up to a day.

## How it is set up

- **Its own user**, `migate-ci`, without sudo, so its Rust toolchains, tools and caches never touch the other projects'
  runners, and they never touch it. The runner is registered to this repository only and runs as a systemd service.
- **PostgreSQL 15 to 18**, installed once from the PostgreSQL project's apt repository with
  `create_main_cluster = false` in `/etc/postgresql-common/createcluster.conf`: binaries only, so no server starts on a
  port the other projects use. There the PostgreSQL jobs find the binaries and install nothing; on GitHub's runners they
  install them each time.
- **Nothing accumulates between jobs.** Every job first runs `.github/actions/runner-prep`: it stops when the shared
  disk has under 10 GB free, and puts temporary files, downloaded tools and cargo's build in the job's temporary
  folder, which the runner empties when the job ends. Cargo prunes its own downloads (unused for three months, since
  Rust 1.88). `runner-cleanup.yml` removes, every Monday, Rust toolchains other than the two CI pins and runner logs
  older than 14 days.
- On GitHub, run logs and artifacts expire on their own (90 days; the mutation reports after 30).
- **Never a fork's code.** The repository is public, and a self-hosted runner runs whatever a job checks out. The
  fallback reruns only this repository's own commits (`head_repository` must be this repository), so a fork's pull
  request that fails on GitHub stays there. A fork's pull request could still edit `ci.yml` to ask for the `migate`
  label, so the repository requires approval before any outside contributor's workflow runs: read the workflow changes
  of such a pull request before approving it.

## Adding a runner

On a Linux machine with python3, sqlite3 and git:

1. `sudo useradd --create-home --shell /bin/bash migate-ci`
2. As that user, unpack the GitHub Actions runner into `~/actions-runner`, then register it to this repository with a
   registration token (`gh api -X POST repos/<owner>/migate/actions/runners/registration-token`):
   `./config.sh --unattended --url https://github.com/<owner>/migate --token <token> --labels migate`
3. `sudo ./svc.sh install migate-ci && sudo ./svc.sh start`
4. PostgreSQL 15 to 18 as above, binaries only.

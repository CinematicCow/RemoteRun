# AGENTS.md

`rr` is a Rust process supervisor/daemon with an embedded React dashboard
(`dashboard/`, built with bun + vite and embedded via `rust-embed`).

## Commands

Run these before committing. Cargo aliases live in `.cargo/config.toml`.

- `cargo check --all-targets` — fast type check
- `cargo lint` — clippy (`--all-targets --all-features`); the hard gate
- `cargo fmt-check` — rustfmt in check mode
- `cargo test-all` — full test suite
- `cargo fmt` — apply formatting

## Lints

Lint policy is in `Cargo.toml` under `[lints.rust]`, `[lints.rustdoc]`, and
`[lints.clippy]`. `clippy::all` plus the focused denies (`panic`, `todo`,
`unimplemented`, `dbg_macro`, `redundant_clone`, ...) are errors; `pedantic`
and `nursery` are warnings. Prefer removing the cause over adding `#[allow]`;
when an allow is warranted, attach it to the narrowest item and say why.

## Building

`build.rs` builds `dashboard/` into `dashboard/dist` with bun. On a machine
without bun, either reuse an existing `dist/` or set
`RR_SKIP_DASHBOARD_BUILD=1` to embed a placeholder page. CI sets this so the
Rust gate doesn't require bun.

## Conventions

- Rust edition 2024, MSRV 1.85.
- Do not add comments unless they explain non-obvious intent.

## VCS

- Commit small, logical groups; one idea per commit.
- Angular style, lowercase: `type: short summary` (`feat`, `fix`,
  `refactor`, `chore`, `docs`, `test`).
- Message max 30 characters, no body, no period.

## Workflow (project tracking)

GitHub is the project management system. `CinematicCow/RemoteRun` issues hold
scope, status, and decision history; PRs hold the code changes. Read both
before doing anything else.

### Session start

1. Read this file, check `git status`, and skim `git log` for recent work.
   Read existing architecture/decision documents and relevant source files;
   do not invent missing architecture documentation.
2. Fetch live GitHub context with `gh`: search open issues across all statuses
   (`gh issue list --state open`), read the relevant issue body and comments
   (`gh issue view N` and `gh issue view N --comments`), and inspect open and
   linked PRs, including their descriptions, reviews, and checks (`gh pr list`,
   `gh pr view N --comments`, `gh pr checks N`). Follow relevant dependencies
   and linked discussions rather than relying on the previous chat.
3. Read milestones (`gh api --paginate repos/CinematicCow/RemoteRun/milestones`)
   and the relevant milestone's issues. Use `gh label` for labels; milestone
   creation/editing uses `gh api` and requires owner approval. Do not invent
   milestones, deadlines, priorities, or example tasks.
4. Summarize what you found before proposing anything. If GitHub access fails
   or context conflicts, report it and ask the owner before proceeding.

OpenCode loads this root `AGENTS.md` in project sessions; fetching GitHub and
reading referenced files are agent responsibilities, not automatic syncing.

### Before writing code

Every unit of work gets an issue first. For a new request:

1. Search open issues for existing work that matches; propose reusing it
   instead of creating a duplicate.
2. Propose the new issue or changes to the existing issue (title, goal,
   acceptance criteria, constraints, test plan) and **wait for the owner's
   approval** before creating/updating its scope or writing code. Explicit
   approval already given for this exact scope need not be requested again.
3. After approval, create or update it with `gh issue create` / `gh issue edit` using
   `.github/ISSUE_TEMPLATE/feature.yml` or `bug.yml` as the shape, label it
   with one type (`feature`, `bug`, `chore`) and `in progress`, then
   start work on a branch named `<type>/<issue#>-slug`.

An issue only earns `status:ready` when its scope, acceptance criteria, and
test plan are filled in. Issues without those stay unlabeled/backlog.

### During work

- Write a comment only when it gives technical value (root cause, design
  choice, blocker data, where the proof is). Do not write status notes,
  approval notes, or test logs. Do not repeat agent chat. Write as
  yourself, short, as in upstream repos. Proof lives in the PR body.
  Check off acceptance criteria in the issue body as they complete.
- Exactly one status label at a time:
  `status:ready` → `in progress` → `status:needs-review`;
  `status:blocked` from any state while waiting on a decision.
- Project-level decisions — scope, behavior, architecture, dependencies,
  priorities, milestones — go to the owner. Do not decide them yourself.
  Routine details that existing conventions already cover are yours.
- Keep changes on the task branch; do not commit or push until the owner says
  the work is done.

### Writing (human-facing text)

Agent-only text (commands, code, local plans) can use normal technical terms.
Everything the human reads — issues, comments, PR bodies, template text, and
summaries to the owner — must use ASD-STE100: one idea per sentence, under
20 words per sentence, active voice, simple words, imperative for steps, no
jargon, no idioms.

### Work is done

When the owner says the work is done, that authorizes: run the checks in
Commands, check off acceptance criteria, commit the intended changes, push the
branch, and open a PR whose body references the issue (`Fixes #N`, see
`.github/PULL_REQUEST_TEMPLATE.md`), set `status:needs-review` on the issue,
and post a completion comment. Never merge; never close the issue directly —
closing happens via the PR merge.

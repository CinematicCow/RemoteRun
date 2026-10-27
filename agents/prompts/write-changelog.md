# Write the changelog

Edit only the `## [Unreleased]` section in `CHANGELOG.md`.
Never touch versioned sections. Never edit any other file.

Read the commits since the last `v*` tag. If no tag exists, read all commits.

Classify each entry into one fixed section, in this order:
`Added`, `Changed`, `Fixed`, `Removed`, `Security`.

Rules:

- Keep one user-facing change per bullet.
- Lead with what users can do or what is fixed.
- Use one short sentence per bullet.
- Keep every pull request number and URL exactly as is.
- Write references as `([#123](https://github.com/CinematicCow/RemoteRun/pull/123))`.
- Drop entries with no user-facing effect, such as CI tweaks, test runner changes, and repo reorganization.
- When in doubt about user impact, keep the entry.
- Never invent behavior, numbers, or compatibility claims.
- Every bullet must trace to a commit or pull request.
- Delete sections left empty.

Leave the `[Unreleased]` heading in place. Do not add a version heading.
`scripts/release.sh` moves the section.

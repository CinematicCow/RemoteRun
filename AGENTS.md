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

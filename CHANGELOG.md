# Changelog

All notable changes to this project are documented here.

## [Unreleased]

### Added

### Changed

### Fixed

### Removed

### Security

## [0.1.1] - 2026-10-28

### Added

- `RR_ALLOWED_HOSTS` lets a reverse proxy use the dashboard. ([#17](https://github.com/CinematicCow/RemoteRun/pull/17))

### Changed

- The dashboard binds to loopback only. Remote access uses Tailscale, SSH, or Cloudflare Tunnel. ([#15](https://github.com/CinematicCow/RemoteRun/pull/15))
- Dashboard control actions need `RR_DASHBOARD_CONTROL=1`. ([#15](https://github.com/CinematicCow/RemoteRun/pull/15))

### Fixed

- The dashboard rejects non-local `Host` and `Origin` headers. ([#17](https://github.com/CinematicCow/RemoteRun/pull/17))
- The TUI shows log history again. ([#2](https://github.com/CinematicCow/RemoteRun/pull/2))

### Security

- The data directory, socket, state, and logs are owner-only. ([#15](https://github.com/CinematicCow/RemoteRun/pull/15))

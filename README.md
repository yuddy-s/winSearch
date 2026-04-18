# WinSearch

Spotlight-style launcher for Windows built with **Tauri 2 + React + TypeScript**.

This repository is currently scoped through **Phase 4 kickoff (Start Menu collector)** from `winsearch-spotlight-plan.md`.

## Product Priority (Locked)

The most important WinSearch outcome is now explicitly file-first:

- very fast file-name search
- file content search (supported text-based files)
- open file directly
- reveal file location in Windows File Explorer

These are treated as must-have release requirements.

Default indexing behavior target:

- first-run full scan of user folders
- incremental scan on app open
- watcher-driven updates
- low-impact gaming-friendly defaults (no heavy always-on background scan)

Current implementation status for this policy:

- first-run full scan of default user folders is wired
- app-open incremental scan of default user folders is wired
- watcher-driven updates are wired with debounce
- UI controls include manual full refresh, pause/resume indexing, and indexing status display

## Quick Start

```bash
npm install
npm run tauri:dev
```

If this is your first clone, run the setup helper:

```bash
npm run setup
```

## Scripts

- `npm run dev` - start Vite frontend on `127.0.0.1:1420`
- `npm run tauri:dev` - start desktop app with Rust backend
- `npm run build` - type-check and build frontend assets
- `npm run tauri:build` - create Tauri production build
- `npm run lint` - run ESLint
- `npm run format` - apply Prettier formatting
- `npm run test` - run Vitest suite
- `npm run check` - lint + test + build

## Project Layout

- `src/` - React UI shell, runtime config, and frontend logging
- `src-tauri/src/` - Rust application entrypoint and Tauri commands
- `src-tauri/src/db/` - SQLite index store and migration runner
- `src-tauri/src/collectors/` - app source collectors (Start Menu currently)
- `src-tauri/migrations/` - SQL migration files for local index schema
- `src-tauri/capabilities/` - Tauri capability permissions
- `scripts/` - local setup helpers
- `docs/` - implementation and engineering conventions

## Current Milestone Status

- Phase 1 complete: repository bootstrap and engineering foundations
- Phase 2 complete: hidden overlay window, global hotkey registration, and focus-safe open/close loop
- Phase 3 foundation complete: SQLite schema, migration path, and initial index store APIs
- Phase 4 started: Start Menu collector plus first filesystem collector/index commands

## Prerequisites

- Node.js and npm
- Rust toolchain (`rustup`, `cargo`, `rustc`) on PATH

Without Rust installed, `npm run tauri:dev` cannot launch the desktop app.

## Environment Strategy

- `.env.development` - dev defaults (verbose logging, relaxed limits)
- `.env.production` - production defaults (reduced logging)
- `.env.example` - template for local overrides

## Logging Conventions

- Frontend logs are routed through `src/lib/logger.ts` and gated by `VITE_LOG_LEVEL`.
- Global handlers in `src/main.tsx` capture uncaught errors and unhandled rejections.
- Rust backend initializes `tracing` in `src-tauri/src/lib.rs` and reads `RUST_LOG` overrides.

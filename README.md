# WinSearch

Spotlight-style launcher for Windows built with **Tauri 2 + React + TypeScript**.

This repository is currently scoped through **Phase 3 foundation (Data Model and Local Index Foundation)** from `winsearch-spotlight-plan.md`.

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
- `src-tauri/migrations/` - SQL migration files for local index schema
- `src-tauri/capabilities/` - Tauri capability permissions
- `scripts/` - local setup helpers
- `docs/` - implementation and engineering conventions

## Current Milestone Status

- Phase 1 complete: repository bootstrap and engineering foundations
- Phase 2 complete: hidden overlay window, global hotkey registration, and focus-safe open/close loop
- Phase 3 foundation complete: SQLite schema, migration path, and initial index store APIs

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

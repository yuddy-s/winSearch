# Phase 1 Foundations

This document defines the baseline engineering standards established in Phase 1.

## Goals Completed

- Tauri + React + TypeScript application shell created.
- Folder structure and script conventions documented.
- Linting, formatting, and testing runners configured.
- Development and production environment strategy added.
- Frontend and backend logging conventions implemented.

## Repo Conventions

- Frontend logic lives in `src/`.
- Rust/Tauri runtime logic lives in `src-tauri/src/`.
- Test setup lives in `src/test/` and test files are colocated by feature.
- Automation helpers live in `scripts/`.

## Config Rules

- Keep shared defaults in `.env.example`.
- Keep committed environment defaults in `.env.development` and `.env.production`.
- Use `.env.local` for machine-specific values and never commit secrets.

## Logging Rules

- Frontend: always use `logger` (`src/lib/logger.ts`) instead of direct console calls.
- Rust: initialize `tracing` once in startup path and use module-level targets.
- Errors should include actionable context (operation + why it failed).

## Exit-Criteria Checklist

- [x] Fresh clone has deterministic setup steps.
- [x] `npm run tauri:dev` starts app shell.
- [x] Lint, test, and build commands exist and are documented.

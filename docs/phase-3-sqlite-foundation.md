# Phase 3 SQLite Foundation

This document captures the Phase 3 foundation work for the WinSearch local index.

## Scope Implemented

- Added a local SQLite-backed index store in `src-tauri/src/db/mod.rs`.
- Added first migration file in `src-tauri/migrations/0001_initial.sql`.
- Added schema versioning using SQLite `PRAGMA user_version`.
- Added baseline app index schema:
  - `app_records`
  - `app_record_sources`
  - `source_versions`
- Added store APIs for:
  - index status lookup
  - app record upsert and list
  - source version set/get
- Wired database initialization into Tauri startup.

## Runtime Behavior

- Database path resolves from `app_local_data_dir` and creates `winsearch.db`.
- Migrations are applied automatically on startup.
- Startup log emits DB path + schema version + app count.

## Tauri Commands Added

- `get_index_status`
- `upsert_index_record`
- `list_index_records`
- `set_index_source_version`
- `get_index_source_version`

These commands are scaffolding for upcoming collectors and search ranking phases.

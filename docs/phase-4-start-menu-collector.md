# Phase 4 Start Menu Collector

This document captures the first Phase 4 collector implementation for WinSearch.

## Scope Implemented

- Added Start Menu collector module at `src-tauri/src/collectors/start_menu.rs`.
- Added shared collection report type at `src-tauri/src/collectors/mod.rs`.
- Collector scans these roots recursively:
  - `%ProgramData%\Microsoft\Windows\Start Menu\Programs`
  - `%APPDATA%\Microsoft\Windows\Start Menu\Programs`
- Collector indexes `.lnk` entries into SQLite via `upsert_app_record`.
- Collector records source version metadata (`start_menu` source version `1`).
- Collector runs once during app startup for initial index hydration.
- Added Tauri command to trigger manual collection: `collect_start_menu_apps`.

## Report Contract

The collection report includes:

- `source`
- `scannedEntries`
- `indexedEntries`
- `skippedEntries`
- `errors`

## Notes

- This is the first Phase 4 slice. Registry and UWP collectors are not implemented yet.
- Launch targets are currently the shortcut paths (`.lnk`) for this foundation pass.

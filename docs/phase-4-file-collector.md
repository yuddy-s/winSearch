# Phase 4 File Collector (Kickoff)

This document captures the first file-system collector slice for the file-first WinSearch scope.

## Scope Implemented

- Added SQLite migration `0002_file_records.sql` with `file_records` table and lookup indexes.
- Added file index DB models and store APIs:
  - `upsert_file_record`
  - `list_files`
  - `search_files`
- Added filesystem collector module: `src-tauri/src/collectors/filesystem.rs`.
- Added Tauri commands:
  - `collect_files_from_paths`
  - `get_default_file_index_roots`
  - `collect_default_user_folders`
  - `upsert_file_index_record`
  - `list_file_index_records`
  - `search_file_index`
- Added startup indexing policy foundation:
  - startup: budgeted incremental scan of default user folders
  - startup collectors execute in a background task so app shell can become interactive sooner
  - baseline marker persisted via `source_versions` key `filesystem_baseline`
- Added debounced filesystem watcher startup service:
  - watches default user roots
  - triggers incremental scan on batched file events
- Added indexing control/status commands:
  - `get_indexing_status`
  - `set_indexing_paused`

## Collector Behavior

- Accepts one or more folder paths and recursively scans files.
- Supports default user-folder presets:
  - `Desktop`, `Documents`, `Downloads`, `Pictures`, `Music`, `Videos` (when present)
  - auto-includes OneDrive root and common OneDrive user folders when available
- Skips common build/noise folders:
  - `.git`, `.idea`, `.vscode`, `node_modules`, `target`, `dist`, `coverage`, `.next`
- Indexes file metadata:
  - name, extension, path, parent path, size, modified timestamp.
- Reads content text for supported text formats when file size is <= 256 KB.
- Stores normalized lowercase content and syncs into an SQLite `FTS5` index for faster content matching.
- Incremental mode skips unchanged files using `size_bytes + modified_at` snapshot checks.
- Incremental mode prunes deleted files from index by comparing scanned paths against indexed paths under roots.
- Startup incremental runs use a file-budget cap so heavy catch-up is deferred to manual full refresh.

## Security Hardening Added

- Reduced IPC surface by exposing fewer mutating index commands.
- Added search query length guardrail for file-content queries.
- Added symlink/reparse-point skip logic during traversal.
- Added canonical visited-directory tracking and max traversal depth.
- Added per-run indexed-file cap to reduce worst-case DoS impact.
- Added explicit root-path validation to reject symlink/reparse roots before traversal begins.
- Escaped SQL `LIKE` wildcard characters when matching indexed paths under user-provided roots.

## Notes

- This is a foundation pass for file search and content search capability.
- More advanced ranking, richer parsers, file watching, and open/reveal UI actions are next steps.

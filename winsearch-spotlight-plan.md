# WinSearch Spotlight-Style Build Plan (Windows)

Date: 2026-04-16  
Status: Active  
Scope: End-to-end plan from idea to release and post-launch

## Product Goal

Build a fast, beautiful, keyboard-first Windows launcher that feels like macOS Spotlight, with **file discovery as the primary use case**: instant file-name search, content-aware search, and one-keystroke open/reveal actions.

## Scope Lock (Must-Have)

The following capabilities are mandatory for V1 and are no longer optional:

- Fast file-name search over user-selected indexed folders.
- File content search (for supported text-based formats).
- Open selected file directly.
- Reveal selected file location in Windows File Explorer.

These are now release-gating requirements.

## North-Star Targets (V1)

- Hotkey to open overlay: perceived instant, <= 700ms cold open.
- Query-to-results response: <= 50ms warm path for typical inputs.
- Content-search response target: <= 150ms warm path for typical queries on indexed files.
- One-keystroke action flow: type -> Enter to open, plus quick reveal-in-Explorer action.
- Idle memory target: <= 150MB.
- Visual quality: polished centered palette with smooth motion and clear hierarchy.

## Proposed Stack

- Desktop shell: `Tauri 2`
- Core engine: `Rust`
- UI: `React + TypeScript`
- Local persistence: `SQLite`
- Build and package: Tauri bundler + CI pipeline

## System Architecture (Target)

- Collector layer: file system crawler (selected folders), plus optional app sources (Start Menu/registry/UWP).
- Index layer: normalize file + app records and persist to SQLite.
- Content layer: text extraction pipeline for searchable file contents.
- Search layer: fuzzy file-name matching + content matching + weighted ranking.
- Action layer: open target and reveal file in Explorer, with usage logging.
- UI layer: Spotlight-like overlay with keyboard-first navigation.

---

## Phase 0 - Product Definition and Success Metrics

### Objective
Lock scope, behavior, and quality bar before coding.

### Work
- Confirm V1 boundary: file-first MVP.
- Define interaction contract: open, type, navigate, launch, close.
- Define acceptance KPIs (latency, memory, relevance).
- Define non-goals for V1 to prevent scope creep.

### Deliverables
- Product requirements section in this plan.
- KPI table used for release gating.

### Exit Criteria
- Team can state what V1 is and what V1 is not in one paragraph.

---

## Phase 1 - Repository Bootstrap and Engineering Foundations

### Objective
Create a stable project skeleton that supports fast iteration.

### Work
- Initialize `Tauri + React + TypeScript` app.
- Set project structure (`src`, `src-tauri/src`, tests, scripts, docs).
- Add baseline linting, formatting, and test runners.
- Add config strategy (`dev`, `prod`) and error logging conventions.

### Deliverables
- Runnable app shell.
- Standard folder conventions and scripts documented.

### Risks
- Build/toolchain friction on Windows.

### Mitigation
- Lock Rust/toolchain versions and keep setup script simple.

### Exit Criteria
- Fresh clone can install and run app locally without manual hacks.

---

## Phase 2 - Overlay Window and Global Hotkey Core

### Objective
Make the app feel alive immediately: hotkey opens a focused centered palette.

### Work
- Create hidden-by-default overlay window.
- Register global hotkey with fallback option on conflicts.
- Ensure focus behavior is consistent (input focused on open).
- Implement close behavior (`Esc`, outside click if enabled).

### Deliverables
- Stable open/close loop via keyboard.
- Hotkey config placeholder for future settings UI.

### Risks
- Hotkey conflicts with existing tools.

### Mitigation
- Fallback hotkey + clear user-facing conflict message.

### Exit Criteria
- 100 repeated hotkey toggles produce no stuck windows or focus loss.

---

## Phase 3 - Data Model and Local Index Foundation

### Objective
Define durable file + app schemas and a query-ready storage layer.

### Work
- Create normalized file schema:
  - `id`, `name`, `extension`, `normalized_path`, `parent_path`, `size_bytes`, `modified_at`.
- Create content-index schema for extracted text and lookup metadata.
- Keep app schema support for launcher parity:
  - `id`, `name`, `aliases`, `source`, `launch_target`, `icon_key`, `last_seen_at`.
- Add SQLite migrations and repository/store APIs.
- Implement upsert strategy to avoid duplicates.
- Build source attribution and version markers for incremental updates.

### Deliverables
- Migration files + data store interface.
- Basic CRUD and query methods for index access.

### Risks
- Duplicate identities across sources.

### Mitigation
- Deterministic merge key rules and source precedence.

### Exit Criteria
- Same app from multiple sources resolves to one canonical record.

---

## Phase 4 - File and App Collectors

### Objective
Populate the index with real files first, then app sources.

### Work
- Implement file collector for user-selected folders with recursive crawl and ignore rules.
- Implement incremental file refresh (change detection by path + mtime/size).
- Implement Start Menu shortcut collector.
- Implement installed program collector from registry.
- Implement UWP package app collector.
- Isolate collector errors so one source failing does not block others.
- Add initial and incremental collection flows.

### Deliverables
- Collector modules with shared trait/interface.
- Collection report object (counts, skipped entries, errors).

### Risks
- Inconsistent metadata quality by source.

### Mitigation
- Source-specific normalization and confidence weighting.

### Exit Criteria
- On representative machines, indexed file coverage is stable and app collectors remain additive.

---

## Phase 5 - Search Engine and Ranking

### Objective
Return the right file quickly and support reliable content hits.

### Work
- Implement tokenizer and normalized search terms.
- Implement file ranking formula:
  - prefix match
  - token match
  - fuzzy/subsequence match
  - content-hit boost
  - usage boost (recency + frequency)
- Add deterministic tie-breakers.
- Add result limits and lightweight query caching.

### Deliverables
- Query API returning ranked file/app results in stable structure.
- Ranking test suite with representative cases.

### Risks
- Perceived relevance mismatch despite fast performance.

### Mitigation
- Configurable scoring weights and rapid tuning loop.

### Exit Criteria
- Top-1 and top-3 relevance meet internal benchmark scenarios.

---

## Phase 6 - Spotlight-Quality UI and Interaction Polish

### Objective
Deliver a polished experience that feels intentional, not generic.

### Work
- Build centered overlay card with depth/backdrop treatment.
- Create input, result list, and item components.
- Keyboard loop support: `Up`, `Down`, `Tab`, `Enter`, `Esc`.
- Add subtle open/close and result transition animation.
- Implement empty/loading/error states.
- Ensure responsive behavior on common desktop resolutions.

### Deliverables
- Production-ready overlay UI.
- Visual and interaction consistency checklist.

### Risks
- UI jank under rapid typing.

### Mitigation
- Debounce only where needed, keep render path minimal, async icon loading.

### Exit Criteria
- Interaction feels smooth at normal typing speed with no dropped focus.

---

## Phase 7 - Launch Actions, Telemetry, and Settings

### Objective
Complete the core loop and prepare for user customization.

### Work
- Implement open-file action runner.
- Implement reveal-in-Explorer action runner.
- Keep app launch action support where relevant.
- Add post-launch usage logging for ranking feedback.
- Add basic settings surface:
  - hotkey override
  - indexed folder management
  - launch at startup toggle
  - clear usage history
- Add safe error feedback for broken launch targets.

### Deliverables
- Reliable action execution with usage update path.
- Minimal settings panel and persisted preferences.

### Risks
- Launch failures for moved or removed targets.

### Mitigation
- Defensive checks and stale-entry cleanup on failures.

### Exit Criteria
- Open, reveal, rerank, and preference persistence work end-to-end.

---

## Phase 8 - Performance, Reliability, and Hardening

### Objective
Hit performance targets consistently, not just on dev hardware.

### Work
- Profile startup, search latency, and memory.
- Optimize hot path allocations in query/ranking.
- Add icon cache and async loading.
- Add DB health checks and recovery flow for corruption.
- Add background incremental index refresh strategy.

### Deliverables
- Performance benchmark scripts.
- Hardening fixes and regression guardrails.

### Risks
- Regressions from feature additions.

### Mitigation
- Track baseline metrics per build and fail when over threshold.

### Exit Criteria
- Meets defined KPI targets on test matrix machines.

---

## Phase 9 - Test Strategy and Quality Gate

### Objective
Prevent regressions and lock in confidence for release.

### Work
- Unit tests: tokenizer, scoring, merge/upsert logic.
- Integration tests: collector -> index -> query pipeline.
- UI tests: keyboard flow and selection behavior.
- End-to-end smoke tests: open -> search -> open file/reveal location.
- Error-path tests: source failure, stale target, empty index.

### Deliverables
- Automated test suites and pass criteria.
- Release quality checklist.

### Exit Criteria
- All required tests pass and critical-path smoke tests are green.

---

## Phase 10 - Packaging, Release, and Distribution

### Objective
Ship a stable installer and first public version.

### Work
- Configure production bundling and app metadata.
- Produce signed installer flow when certs are available.
- Add release notes template and changelog process.
- Validate install, update, and uninstall behaviors.

### Deliverables
- Installable release artifact.
- Release notes and basic support instructions.

### Exit Criteria
- Clean install and first-run success on test matrix.

---

## Phase 11 - Post-Launch Monitoring and Iteration Loop

### Objective
Improve relevance and stability based on real use.

### Work
- Track user-reported misses and launch failures.
- Tune ranking weights based on real query patterns.
- Improve collector coverage for edge environments.
- Prioritize fixes from top pain buckets.

### Deliverables
- Weekly tuning log and patch release cadence.
- Documented known issues and fixes.

### Exit Criteria
- Clear downward trend in relevance complaints and launch errors.

---

## Phase 12 - V2 Expansion (Files and Rich Actions)

### Objective
Expand from file-first launcher to broader Spotlight equivalent.

### V2 Work Candidates
- Search categories and filters.
- Calculator/math and quick web fallback.
- Optional plugin/action framework.

### V2 Guardrails
- Keep app-launch speed unaffected.
- Feature flags for heavier indexing features.
- Preserve keyboard-first interaction contract.

---

## Cross-Phase Milestones

- Milestone A: Core shell works (Phases 1-2 complete).
- Milestone B: Real searchable file index with content hits (Phases 3-5 complete).
- Milestone C: Polished UX + reliable open/reveal loop (Phases 6-7 complete).
- Milestone D: Release candidate quality (Phases 8-10 complete).
- Milestone E: Growth and expansion (Phases 11-12 active).

## Suggested First Execution Slice

To start implementation immediately with high momentum:

1. Phase 1 bootstrap.
2. Phase 2 hotkey + overlay shell.
3. Phase 3 file-first schema + SQLite store.
4. Phase 4 file collector first (then Start Menu/registry/UWP).
5. Phase 5 file-name + content ranking baseline.

This gets to a usable MVP fastest while keeping room for polish and expansion.

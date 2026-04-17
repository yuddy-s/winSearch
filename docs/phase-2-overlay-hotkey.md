# Phase 2 Overlay + Hotkey Core

This document captures the Phase 2 implementation for WinSearch.

## Scope Implemented

- Main window is configured as a hidden-by-default overlay.
- Global hotkey registration is wired at startup.
- Hotkey fallback logic is implemented when the primary combination is unavailable.
- Focus is restored to the search input on overlay open.
- Overlay closes on `Esc` and outside click.
- Settings placeholder for hotkey customization is visible in the UI.

## Hotkey Strategy

- Preferred: `Alt+Space`
- Fallback: `Ctrl+Shift+Space`
- Conflict details are captured and displayed in the overlay.

## Event Contract

- `winsearch://overlay-opened` - emitted when overlay becomes visible.
- `winsearch://overlay-closed` - emitted when overlay is hidden.

## Commands Added

- `hide_overlay`
- `show_overlay`
- `get_hotkey_status`

These commands and events are temporary interface points that can later be consumed by a dedicated settings screen.

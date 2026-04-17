import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { logger } from "./lib/logger";

type HotkeyStatus = {
  activeShortcut: string | null;
  failedShortcuts: string[];
  fallbackInUse: boolean;
};

type SearchItem = {
  id: string;
  name: string;
  hint: string;
};

const SAMPLE_RESULTS: SearchItem[] = [
  { id: "code", name: "Visual Studio Code", hint: "Development" },
  { id: "terminal", name: "Windows Terminal", hint: "System" },
  { id: "chrome", name: "Google Chrome", hint: "Browser" },
  { id: "settings", name: "Windows Settings", hint: "System" },
  { id: "notion", name: "Notion", hint: "Productivity" },
];

const filterResults = (query: string): SearchItem[] => {
  if (!query.trim()) {
    return SAMPLE_RESULTS;
  }

  const normalized = query.toLowerCase();

  return SAMPLE_RESULTS.filter((item) => item.name.toLowerCase().includes(normalized));
};

const isTauriRuntime = (): boolean => {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
};

function App() {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [hotkeyStatus, setHotkeyStatus] = useState<HotkeyStatus>({
    activeShortcut: null,
    failedShortcuts: [],
    fallbackInUse: false,
  });

  const results = useMemo(() => {
    return filterResults(query);
  }, [query]);

  useEffect(() => {
    if (!isTauriRuntime()) {
      inputRef.current?.focus();
      return;
    }

    const syncHotkeyStatus = async () => {
      try {
        const status = await invoke<HotkeyStatus>("get_hotkey_status");
        setHotkeyStatus(status);
      } catch (error) {
        logger.warn("Failed to load hotkey status", error);
      }
    };

    const focusInput = () => {
      window.setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 16);
    };

    const unlistenOpenPromise = listen("winsearch://overlay-opened", () => {
      focusInput();
    });

    const unlistenClosedPromise = listen("winsearch://overlay-closed", () => {
      setQuery("");
      setSelectedIndex(0);
    });

    syncHotkeyStatus();
    focusInput();

    return () => {
      unlistenOpenPromise
        .then((unlisten) => unlisten())
        .catch((error) => logger.warn("Failed to remove overlay-opened listener", error));
      unlistenClosedPromise
        .then((unlisten) => unlisten())
        .catch((error) => logger.warn("Failed to remove overlay-closed listener", error));
    };
  }, []);

  useEffect(() => {
    if (selectedIndex > results.length - 1) {
      setSelectedIndex(0);
    }
  }, [results, selectedIndex]);

  const closeOverlay = async () => {
    try {
      await invoke("hide_overlay");
    } catch (error) {
      logger.warn("Failed to close overlay", error);
    }
  };

  const launchSelection = async () => {
    const active = results[selectedIndex];

    if (!active) {
      return;
    }

    logger.info("Phase 2 launch placeholder selected", active);
    await closeOverlay();
  };

  const onInputKeyDown = async (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      await closeOverlay();
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (results.length > 0) {
        setSelectedIndex((index) => (index + 1) % results.length);
      }
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      if (results.length > 0) {
        setSelectedIndex((index) => (index - 1 + results.length) % results.length);
      }
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      await launchSelection();
    }
  };

  return (
    <main
      className="overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          void closeOverlay();
        }
      }}
    >
      <section className="palette" role="dialog" aria-label="WinSearch overlay">
        <header className="palette-header">
          <h1>WinSearch</h1>
          <span className="hotkey-badge">{hotkeyStatus.activeShortcut ?? "No hotkey"}</span>
        </header>
        <label className="search-input-wrap" htmlFor="search-input">
          <span className="sr-only">Search applications</span>
          <input
            id="search-input"
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedIndex(0);
            }}
            onKeyDown={(event) => {
              void onInputKeyDown(event);
            }}
            placeholder="Search apps"
            autoComplete="off"
            spellCheck={false}
          />
        </label>

        <ul className="results" role="listbox" aria-label="Application search results">
          {results.length === 0 ? (
            <li className="empty-state">No matches yet. Keep typing.</li>
          ) : (
            results.map((item, index) => (
              <li
                className={index === selectedIndex ? "result-row is-selected" : "result-row"}
                key={item.id}
                role="option"
                aria-selected={index === selectedIndex}
              >
                <span>{item.name}</span>
                <small>{item.hint}</small>
              </li>
            ))
          )}
        </ul>

        <div className="footer-row">
          <span>Enter to launch</span>
          <span>Esc or outside click to close</span>
          <span>Settings UI placeholder: hotkey override in Phase 7</span>
        </div>

        {hotkeyStatus.fallbackInUse ? (
          <p className="notice">Primary hotkey was busy, fallback is active.</p>
        ) : null}

        {hotkeyStatus.failedShortcuts.length > 0 ? (
          <p className="notice">Conflicts: {hotkeyStatus.failedShortcuts.join(" | ")}</p>
        ) : null}
      </section>
    </main>
  );
}

export default App;

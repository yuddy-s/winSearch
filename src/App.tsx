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

type FileRecord = {
  id: string;
  name: string;
  extension: string | null;
  normalizedName: string;
  normalizedPath: string;
  parentPath: string;
  sizeBytes: number;
  modifiedAt: number;
  contentIndexed: boolean;
  lastSeenAt: number;
  createdAt: number;
  updatedAt: number;
};

type CollectionReport = {
  source: string;
  mode: string | null;
  scannedEntries: number;
  indexedEntries: number;
  skippedEntries: number;
  prunedEntries: number;
  errors: string[];
};

type IndexingScanSummary = {
  mode: string;
  reason: string;
  scannedEntries: number;
  indexedEntries: number;
  skippedEntries: number;
  prunedEntries: number;
  errorCount: number;
  completedAt: number;
};

type IndexingStatus = {
  paused: boolean;
  watcherEnabled: boolean;
  baselineComplete: boolean;
  scanInProgress: boolean;
  defaultRoots: string[];
  lastScan: IndexingScanSummary | null;
  lastError: string | null;
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
  const [isManualRefreshLoading, setIsManualRefreshLoading] = useState(false);
  const [isPauseToggleLoading, setIsPauseToggleLoading] = useState(false);
  const [manualRefreshNotice, setManualRefreshNotice] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [indexingStatus, setIndexingStatus] = useState<IndexingStatus | null>(null);
  const [fileResults, setFileResults] = useState<FileRecord[]>([]);
  const [hotkeyStatus, setHotkeyStatus] = useState<HotkeyStatus>({
    activeShortcut: null,
    failedShortcuts: [],
    fallbackInUse: false,
  });

  const isTauri = isTauriRuntime();

  const sampleResults = useMemo(() => {
    return filterResults(query);
  }, [query]);

  const results = isTauri ? fileResults : sampleResults;

  useEffect(() => {
    if (!isTauri) {
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

    const syncIndexingStatus = async () => {
      try {
        const status = await invoke<IndexingStatus>("get_indexing_status");
        setIndexingStatus(status);
      } catch (error) {
        logger.warn("Failed to load indexing status", error);
      }
    };

    const loadInitialResults = async () => {
      try {
        const initialResults = await invoke<FileRecord[]>("list_file_index_records", { limit: 50 });
        setFileResults(initialResults);
      } catch (error) {
        logger.warn("Failed to load initial file results", error);
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
      setActionNotice(null);
    });

    const unlistenIndexingStatusPromise = listen<IndexingStatus>("winsearch://indexing-status", (event) => {
      setIndexingStatus(event.payload);
    });

    const indexingStatusInterval = window.setInterval(() => {
      void syncIndexingStatus();
    }, 5000);

    syncHotkeyStatus();
    syncIndexingStatus();
    loadInitialResults();
    focusInput();

    return () => {
      window.clearInterval(indexingStatusInterval);
      unlistenOpenPromise
        .then((unlisten) => unlisten())
        .catch((error) => logger.warn("Failed to remove overlay-opened listener", error));
      unlistenClosedPromise
        .then((unlisten) => unlisten())
        .catch((error) => logger.warn("Failed to remove overlay-closed listener", error));
      unlistenIndexingStatusPromise
        .then((unlisten) => unlisten())
        .catch((error) => logger.warn("Failed to remove indexing-status listener", error));
    };
  }, [isTauri]);

  useEffect(() => {
    if (selectedIndex > results.length - 1) {
      setSelectedIndex(0);
    }
  }, [results, selectedIndex]);

  useEffect(() => {
    if (!isTauri) {
      return;
    }

    let isCancelled = false;
    const timerId = window.setTimeout(async () => {
      try {
        if (!query.trim()) {
          const latestFiles = await invoke<FileRecord[]>("list_file_index_records", { limit: 50 });
          if (!isCancelled) {
            setFileResults(latestFiles);
          }
          return;
        }

        const searchedFiles = await invoke<FileRecord[]>("search_file_index", { query, limit: 50 });
        if (!isCancelled) {
          setFileResults(searchedFiles);
        }
      } catch (error) {
        logger.warn("Failed to query file index", error);
      }
    }, 120);

    return () => {
      isCancelled = true;
      window.clearTimeout(timerId);
    };
  }, [isTauri, query]);

  const closeOverlay = async () => {
    try {
      await invoke("hide_overlay");
    } catch (error) {
      logger.warn("Failed to close overlay", error);
    }
  };

  const launchSelection = async () => {
    if (!isTauri) {
      const active = results[selectedIndex];
      if (!active) {
        return;
      }
      logger.info("Launch placeholder selected", active);
      await closeOverlay();
      return;
    }

    const active = fileResults[selectedIndex];
    if (!active) {
      return;
    }

    try {
      await invoke("open_file_index_record", { fileId: active.id });
      setActionNotice(null);
      await closeOverlay();
    } catch (error) {
      logger.warn("Failed to open selected file", error);
      setActionNotice("Could not open that file. It may have been moved or deleted.");
      await refreshResults();
    }
  };

  const revealSelectionInExplorer = async () => {
    if (!isTauri) {
      return;
    }

    const active = fileResults[selectedIndex];
    if (!active) {
      return;
    }

    try {
      await invoke("reveal_file_index_record", { fileId: active.id });
      setActionNotice(null);
      await closeOverlay();
    } catch (error) {
      logger.warn("Failed to reveal selected file in Explorer", error);
      setActionNotice("Could not reveal that file. It may have been moved or deleted.");
      await refreshResults();
    }
  };

  const refreshResults = async () => {
    if (!isTauri) {
      return;
    }

    try {
      if (!query.trim()) {
        const latestFiles = await invoke<FileRecord[]>("list_file_index_records", { limit: 50 });
        setFileResults(latestFiles);
      } else {
        const searchedFiles = await invoke<FileRecord[]>("search_file_index", { query, limit: 50 });
        setFileResults(searchedFiles);
      }
    } catch (error) {
      logger.warn("Failed to refresh file results", error);
    }
  };

  const runManualFullRefresh = async () => {
    if (!isTauri) {
      return;
    }

    setIsManualRefreshLoading(true);
    setManualRefreshNotice(null);

    try {
      const report = await invoke<CollectionReport>("collect_default_user_folders", { incremental: false });
      setManualRefreshNotice(
        `Refresh complete: scanned ${report.scannedEntries}, indexed ${report.indexedEntries}, pruned ${report.prunedEntries}`,
      );
      await refreshResults();
    } catch (error) {
      logger.warn("Manual full refresh failed", error);
      setManualRefreshNotice("Manual refresh failed. Check logs for details.");
    } finally {
      setIsManualRefreshLoading(false);
    }
  };

  const togglePauseIndexing = async () => {
    if (!isTauri || !indexingStatus) {
      return;
    }

    setIsPauseToggleLoading(true);
    try {
      const nextStatus = await invoke<IndexingStatus>("set_indexing_paused", { paused: !indexingStatus.paused });
      setIndexingStatus(nextStatus);
    } catch (error) {
      logger.warn("Failed to toggle indexing pause", error);
    } finally {
      setIsPauseToggleLoading(false);
    }
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

    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      await revealSelectionInExplorer();
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
          <span className="sr-only">Search files</span>
          <input
            id="search-input"
            ref={inputRef}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setSelectedIndex(0);
              setActionNotice(null);
            }}
            onKeyDown={(event) => {
              void onInputKeyDown(event);
            }}
            placeholder="Search files"
            autoComplete="off"
            spellCheck={false}
          />
        </label>

        {isTauri ? (
          <div className="control-row">
            <button
              type="button"
              className="control-btn"
              disabled={isManualRefreshLoading}
              onClick={() => {
                void runManualFullRefresh();
              }}
            >
              {isManualRefreshLoading ? "Refreshing..." : "Manual Full Refresh"}
            </button>
            <button
              type="button"
              className="control-btn secondary"
              disabled={isPauseToggleLoading}
              onClick={() => {
                void togglePauseIndexing();
              }}
            >
              {isPauseToggleLoading
                ? "Updating..."
                : indexingStatus?.paused
                  ? "Resume Indexing"
                  : "Pause Indexing"}
            </button>
          </div>
        ) : null}

        {indexingStatus ? (
          <div className="status-row">
            <span>{indexingStatus.paused ? "Indexing paused" : "Indexing active"}</span>
            <span>{indexingStatus.watcherEnabled ? "Watcher on" : "Watcher off"}</span>
            <span>{indexingStatus.scanInProgress ? "Scan in progress" : "Idle"}</span>
            <span>{indexingStatus.baselineComplete ? "Baseline complete" : "Baseline pending"}</span>
          </div>
        ) : null}

        {indexingStatus?.lastScan ? (
          <p className="notice">
            Last scan ({indexingStatus.lastScan.reason}/{indexingStatus.lastScan.mode}): scanned{" "}
            {indexingStatus.lastScan.scannedEntries}, indexed {indexingStatus.lastScan.indexedEntries}, pruned{" "}
            {indexingStatus.lastScan.prunedEntries}
          </p>
        ) : null}

        {manualRefreshNotice ? <p className="notice">{manualRefreshNotice}</p> : null}
        {actionNotice ? <p className="notice">{actionNotice}</p> : null}
        {indexingStatus?.lastError ? <p className="notice">Indexing note: {indexingStatus.lastError}</p> : null}

        <ul className="results" role="listbox" aria-label="File search results">
          {results.length === 0 ? (
            <li className="empty-state">No matches yet. Keep typing.</li>
          ) : (
            results.map((item, index) => (
              <li
                className={index === selectedIndex ? "result-row is-selected" : "result-row"}
                key={item.id}
                role="option"
                aria-selected={index === selectedIndex}
                onMouseEnter={() => {
                  setSelectedIndex(index);
                }}
                onDoubleClick={() => {
                  if (isTauri) {
                    void launchSelection();
                  }
                }}
              >
                <span>{item.name}</span>
                <small>{isTauri ? (item as FileRecord).parentPath : (item as SearchItem).hint}</small>
              </li>
            ))
          )}
        </ul>

        <div className="footer-row">
          <span>Enter to open file</span>
          <span>Ctrl+Enter to reveal in Explorer</span>
          <span>Esc or outside click to close</span>
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

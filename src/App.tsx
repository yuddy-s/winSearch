import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { runtimeConfig } from "./config/runtime";
import { logger } from "./lib/logger";

function App() {
  const [backendStatus, setBackendStatus] = useState("Not checked");

  const checkBackend = async () => {
    try {
      const value = await invoke<string>("ping");
      setBackendStatus(value);
    } catch (error) {
      logger.warn("Backend ping failed (expected in browser mode)", error);
      setBackendStatus("Unavailable outside Tauri runtime");
    }
  };

  return (
    <main className="shell">
      <section className="card" role="region" aria-label="WinSearch foundation status">
        <p className="eyebrow">Phase 1 Foundation</p>
        <h1>WinSearch</h1>
        <p>
          Tauri + React + TypeScript shell is online with linting, formatting, and tests wired for
          fast iteration.
        </p>
        <div className="status-row">
          <span>Mode: {runtimeConfig.mode}</span>
          <span>Log level: {runtimeConfig.logLevel}</span>
          <span>Result limit: {runtimeConfig.resultsLimit}</span>
        </div>
        <div className="status-row">
          <button type="button" onClick={checkBackend}>
            Ping Rust backend
          </button>
          <span>Backend: {backendStatus}</span>
        </div>
      </section>
    </main>
  );
}

export default App;

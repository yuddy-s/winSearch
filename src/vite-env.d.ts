/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_LOG_LEVEL?: "debug" | "info" | "warn" | "error";
  readonly VITE_RESULTS_LIMIT?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

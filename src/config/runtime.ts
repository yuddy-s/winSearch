const mode = import.meta.env.MODE;

const normalizeResultLimit = (rawValue: string | undefined): number => {
  const parsed = Number(rawValue);

  if (!Number.isFinite(parsed) || parsed < 1) {
    return 8;
  }

  return Math.floor(parsed);
};

export const runtimeConfig = Object.freeze({
  mode,
  isDev: import.meta.env.DEV,
  isProd: import.meta.env.PROD,
  logLevel: import.meta.env.VITE_LOG_LEVEL ?? (import.meta.env.DEV ? "debug" : "info"),
  resultsLimit: normalizeResultLimit(import.meta.env.VITE_RESULTS_LIMIT),
});

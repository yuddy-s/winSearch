import { runtimeConfig } from "../config/runtime";

type LogLevel = "debug" | "info" | "warn" | "error";

const levelOrder: Record<LogLevel, number> = {
  debug: 10,
  info: 20,
  warn: 30,
  error: 40,
};

const configuredLevel: LogLevel =
  runtimeConfig.logLevel === "debug" ||
  runtimeConfig.logLevel === "info" ||
  runtimeConfig.logLevel === "warn" ||
  runtimeConfig.logLevel === "error"
    ? runtimeConfig.logLevel
    : "info";

const shouldLog = (eventLevel: LogLevel): boolean => {
  return levelOrder[eventLevel] >= levelOrder[configuredLevel];
};

const formatMessage = (message: string): string => {
  return `[WinSearch:${runtimeConfig.mode}] ${message}`;
};

export const logger = {
  debug(message: string, payload?: unknown): void {
    if (!shouldLog("debug")) {
      return;
    }

    console.debug(formatMessage(message), payload ?? "");
  },
  info(message: string, payload?: unknown): void {
    if (!shouldLog("info")) {
      return;
    }

    console.info(formatMessage(message), payload ?? "");
  },
  warn(message: string, payload?: unknown): void {
    if (!shouldLog("warn")) {
      return;
    }

    console.warn(formatMessage(message), payload ?? "");
  },
  error(message: string, payload?: unknown): void {
    if (!shouldLog("error")) {
      return;
    }

    console.error(formatMessage(message), payload ?? "");
  },
};

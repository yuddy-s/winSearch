import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { logger } from "./lib/logger";
import "./styles.css";

window.addEventListener("error", (event) => {
  logger.error("Unhandled window error", event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  logger.error("Unhandled promise rejection", event.reason);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

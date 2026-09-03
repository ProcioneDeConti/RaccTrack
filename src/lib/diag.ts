// Forwards webview console errors, uncaught exceptions and CSP violations to the
// Rust log so they show up in the dev terminal.

import { invoke } from "@tauri-apps/api/core";

function stringify(a: unknown): string {
  if (a instanceof Error) return a.stack ?? `${a.name}: ${a.message}`;
  if (typeof a === "object" && a !== null) {
    try {
      return JSON.stringify(a);
    } catch {
      return String(a);
    }
  }
  return String(a);
}

function fwd(level: "error" | "warn" | "info", ...args: unknown[]) {
  const message = args.map(stringify).join(" ").slice(0, 2000);
  invoke("log_frontend", { level, message }).catch(() => {});
}

export function installDiagnostics() {
  const origError = console.error.bind(console);
  console.error = (...a: unknown[]) => {
    origError(...a);
    fwd("error", ...a);
  };
  const origWarn = console.warn.bind(console);
  console.warn = (...a: unknown[]) => {
    origWarn(...a);
    fwd("warn", ...a);
  };

  window.addEventListener("error", (e) => {
    fwd("error", "uncaught", e.message, `${e.filename}:${e.lineno}:${e.colno}`);
  });
  window.addEventListener("unhandledrejection", (e) => {
    fwd("error", "unhandledrejection", e.reason);
  });
  document.addEventListener("securitypolicyviolation", (e) => {
    fwd(
      "error",
      "CSP violation:",
      e.violatedDirective,
      "blocked",
      e.blockedURI,
      "from",
      `${e.sourceFile}:${e.lineNumber}`,
    );
  });

  fwd("info", "diagnostics installed");
}

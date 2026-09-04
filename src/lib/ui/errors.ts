// Turn a backend/Rust error string (or any thrown value) into something worth
// showing a user. The Tauri command layer hands us `Err(String)`, so most of
// these are `anyhow` chains like "adsb.fi: request to https://… error sending
// request" — readable-ish but noisy.

export function humanizeError(e: unknown): string {
  const raw = (e instanceof Error ? e.message : String(e ?? "")).trim();
  if (!raw) return "Something went wrong.";
  const low = raw.toLowerCase();

  if (
    low.includes("429") ||
    low.includes("rate limit") ||
    low.includes("too many requests")
  )
    return "The data source is rate-limiting us — try again in a minute.";

  if (low.includes("timed out") || low.includes("timeout"))
    return "The request timed out — the source may be slow or unreachable.";

  if (
    low.includes("error sending request") ||
    low.includes("dns error") ||
    low.includes("failed to lookup") ||
    low.includes("connection refused") ||
    low.includes("unreachable") ||
    low.includes("network")
  )
    return "Couldn't reach the data source — check your connection.";

  if (low.includes("no live data") || low.includes("not in view"))
    return "No live data for this aircraft right now.";

  if (/\bhttp [45]\d\d\b/.test(low) || /\bstatus [45]\d\d\b/.test(low))
    return "The data source returned an error.";

  if (
    low.includes("decoding") ||
    low.includes("invalid json") ||
    low.includes("expected value") ||
    low.includes("missing field")
  )
    return "The data source returned something unexpected.";

  // Fallback: drop URLs, collapse whitespace, cap length.
  const cleaned = raw
    .replace(/https?:\/\/\S+/g, "")
    .replace(/\s{2,}/g, " ")
    .trim();
  if (!cleaned) return "Something went wrong.";
  return cleaned.length > 140 ? `${cleaned.slice(0, 137)}…` : cleaned;
}

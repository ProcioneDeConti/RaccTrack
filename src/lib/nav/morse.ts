// International Morse for navaid idents — used to show what a VOR/NDB keys as
// its audio identifier (and, in Phase 2, to check the decoded ident against
// the expected one).

const TABLE: Record<string, string> = {
  A: ".-", B: "-...", C: "-.-.", D: "-..", E: ".", F: "..-.", G: "--.",
  H: "....", I: "..", J: ".---", K: "-.-", L: ".-..", M: "--", N: "-.",
  O: "---", P: ".--.", Q: "--.-", R: ".-.", S: "...", T: "-", U: "..-",
  V: "...-", W: ".--", X: "-..-", Y: "-.--", Z: "--..",
  "0": "-----", "1": ".----", "2": "..---", "3": "...--", "4": "....-",
  "5": ".....", "6": "-....", "7": "--...", "8": "---..", "9": "----.",
};

/** Morse for each character of `ident`, e.g. "PDZ" → ["·−−·", "−··", "−−··"].
 *  Uses the middle-dot / minus glyphs so it reads well in a chart-style label. */
export function morseFor(ident: string): string[] {
  return ident
    .toUpperCase()
    .split("")
    .map((ch) => TABLE[ch] ?? "")
    .filter((s) => s.length > 0)
    .map((s) => s.replace(/\./g, "·").replace(/-/g, "−"));
}

/** Single string with characters separated by a thin gap, for a one-line label. */
export function morseLine(ident: string): string {
  return morseFor(ident).join(" ");
}

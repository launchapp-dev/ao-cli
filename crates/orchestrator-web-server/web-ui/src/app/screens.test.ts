import { describe, expect, it } from "vitest";

import { matchesConfirmationPhrase } from "./screens";

describe("matchesConfirmationPhrase", () => {
  it("matches exact phrase with trim-only normalization", () => {
    expect(matchesConfirmationPhrase("  STOP DAEMON  ", "STOP DAEMON")).toBe(true);
  });

  it("enforces case-sensitive matching", () => {
    expect(matchesConfirmationPhrase("stop daemon", "STOP DAEMON")).toBe(false);
  });

  it("does not normalize internal whitespace", () => {
    expect(matchesConfirmationPhrase("CLEAR  DAEMON LOGS", "CLEAR DAEMON LOGS")).toBe(false);
  });
});

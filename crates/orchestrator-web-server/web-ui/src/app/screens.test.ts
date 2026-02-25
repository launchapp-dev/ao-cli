import { describe, expect, it } from "vitest";

import { matchesConfirmationPhrase } from "./screens";

describe("matchesConfirmationPhrase", () => {
  it("matches phrase case-insensitively", () => {
    expect(matchesConfirmationPhrase("stop", "STOP")).toBe(true);
  });

  it("normalizes whitespace in user input", () => {
    expect(matchesConfirmationPhrase("  clear   logs  ", "CLEAR LOGS")).toBe(true);
  });

  it("rejects non-matching confirmation input", () => {
    expect(matchesConfirmationPhrase("clear", "CLEAR LOGS")).toBe(false);
  });
});

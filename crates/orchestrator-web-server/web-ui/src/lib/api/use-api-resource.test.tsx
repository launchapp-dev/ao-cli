// @vitest-environment jsdom

import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ApiResult } from "./envelope";
import { useApiResource } from "./use-api-resource";

describe("useApiResource", () => {
  it("maps thrown request failures to error state", async () => {
    const request = async (): Promise<ApiResult<{ value: string }>> => {
      throw new Error("exploded request");
    };

    render(<ResourceProbe request={request} />);

    await waitFor(() => {
      expect(screen.getByTestId("status").textContent).toBe("error");
    });

    expect(screen.getByTestId("error-code").textContent).toBe("resource_request_failed");
    expect(screen.getByTestId("error-message").textContent).toContain("exploded request");
  });
});

function ResourceProbe(props: {
  request: () => Promise<ApiResult<{ value: string }>>;
}) {
  const state = useApiResource(props.request, [props.request]);

  return (
    <div>
      <span data-testid="status">{state.status}</span>
      <span data-testid="error-code">{state.status === "error" ? state.error.code : ""}</span>
      <span data-testid="error-message">{state.status === "error" ? state.error.message : ""}</span>
    </div>
  );
}

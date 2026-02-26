// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { ActionGateDialog, type ActionGateConfig } from "./action-gate-dialog";

const gateConfig: ActionGateConfig = {
  actionKey: "workflows.cancel",
  targetId: "wf-1",
  confirmationPhrase: "CANCEL wf-1",
  impactSummary: "Cancelling wf-1 interrupts active execution.",
  submitLabel: "Confirm Workflow Cancellation",
};

function ActionGateHarness(props: { onConfirm?: () => void } = {}) {
  const [gate, setGate] = useState<ActionGateConfig | null>(null);

  return (
    <div>
      <button type="button" onClick={() => setGate(gateConfig)}>
        Open gate
      </button>
      <ActionGateDialog
        gate={gate}
        pending={false}
        onClose={() => setGate(null)}
        onConfirm={() => {
          props.onConfirm?.();
          setGate(null);
        }}
      />
    </div>
  );
}

describe("ActionGateDialog", () => {
  it("restores focus to the trigger after closing", async () => {
    render(<ActionGateHarness />);

    const trigger = screen.getByRole("button", { name: "Open gate" });
    trigger.focus();
    fireEvent.click(trigger);

    const confirmationInput = await screen.findByLabelText("Confirmation phrase");
    await waitFor(() => {
      expect(document.activeElement).toBe(confirmationInput);
    });

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => {
      expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(document.activeElement).toBe(trigger);
  });

  it("requires the confirmation phrase before enabling submit", async () => {
    const onConfirm = vi.fn();
    render(<ActionGateHarness onConfirm={onConfirm} />);

    fireEvent.click(screen.getByRole("button", { name: "Open gate" }));

    const confirmationInput = await screen.findByLabelText("Confirmation phrase");
    const confirmButton = screen.getByRole("button", {
      name: "Confirm Workflow Cancellation",
    });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(confirmationInput, {
      target: { value: "cancel" },
    });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(true);

    fireEvent.change(confirmationInput, {
      target: { value: "  cancel   WF-1  " },
    });
    expect((confirmButton as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(confirmButton);
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});

// @vitest-environment jsdom

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AppShellLayout } from "./shell";

vi.mock("./project-context", () => ({
  ProjectContextProvider: ({ children }: { children: ReactNode }) => children,
  useProjectContext: () => ({
    activeProjectId: null,
    source: "none",
    projects: [],
    setActiveProjectId: vi.fn(),
  }),
}));

type MatchMediaController = {
  setMatches: (nextValue: boolean) => void;
};

describe("AppShellLayout keyboard and responsive behavior", () => {
  beforeEach(() => {
    Object.defineProperty(window, "scrollTo", {
      configurable: true,
      value: vi.fn(),
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("keeps primary navigation inert in compact mode until menu opens and restores focus on Escape", async () => {
    installMatchMedia(true);
    renderShell();

    const menuButton = screen.getByRole("button", { name: "Open primary navigation" });
    const primaryNav = getPrimaryNav();
    const firstNavLink = screen.getByRole("link", { name: "Dashboard", hidden: true });

    expect(primaryNav.getAttribute("aria-hidden")).toBe("true");
    expect(firstNavLink.getAttribute("tabindex")).toBe("-1");

    fireEvent.click(menuButton);

    await waitFor(() => {
      expect(primaryNav.hasAttribute("aria-hidden")).toBe(false);
      expect(firstNavLink.getAttribute("tabindex")).toBeNull();
    });
    expect(document.activeElement).toBe(firstNavLink);

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(menuButton.getAttribute("aria-expanded")).toBe("false");
      expect(primaryNav.getAttribute("aria-hidden")).toBe("true");
      expect(document.activeElement).toBe(menuButton);
    });
  });

  it("traps tab focus within the open compact navigation menu", async () => {
    installMatchMedia(true);
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "Open primary navigation" }));

    const firstNavLink = screen.getByRole("link", { name: "Dashboard", hidden: true });
    const lastNavLink = screen.getByRole("link", { name: "Review Handoff", hidden: true });

    await waitFor(() => {
      expect(document.activeElement).toBe(firstNavLink);
    });

    lastNavLink.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(firstNavLink);

    firstNavLink.focus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(lastNavLink);
  });

  it("moves focus into compact navigation when tabbing from outside the nav", async () => {
    installMatchMedia(true);
    renderShell();

    const menuButton = screen.getByRole("button", { name: "Open primary navigation" });
    fireEvent.click(menuButton);

    const firstNavLink = screen.getByRole("link", { name: "Dashboard", hidden: true });
    await waitFor(() => {
      expect(document.activeElement).toBe(firstNavLink);
    });

    menuButton.focus();
    expect(document.activeElement).toBe(menuButton);

    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(firstNavLink);
  });

  it("closes the mobile menu when viewport changes from compact to wide", async () => {
    const mediaQuery = installMatchMedia(true);
    renderShell();

    const menuButton = screen.getByRole("button", { name: "Open primary navigation" });
    const primaryNav = getPrimaryNav();
    const firstNavLink = screen.getByRole("link", { name: "Dashboard", hidden: true });

    fireEvent.click(menuButton);
    await waitFor(() => {
      expect(menuButton.getAttribute("aria-expanded")).toBe("true");
    });

    act(() => {
      mediaQuery.setMatches(false);
    });

    await waitFor(() => {
      expect(menuButton.getAttribute("aria-expanded")).toBe("false");
      expect(primaryNav.hasAttribute("aria-hidden")).toBe(false);
      expect(firstNavLink.getAttribute("tabindex")).toBeNull();
    });
    expect(screen.queryByRole("button", { name: "Close navigation menu" })).toBeNull();
  });

  it("locks body scroll while compact navigation is open and restores it on close", async () => {
    installMatchMedia(true);
    document.body.style.overflow = "auto";
    renderShell();

    fireEvent.click(screen.getByRole("button", { name: "Open primary navigation" }));
    await waitFor(() => {
      expect(document.body.style.overflow).toBe("hidden");
    });

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => {
      expect(document.body.style.overflow).toBe("auto");
    });
  });
});

function renderShell() {
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: <AppShellLayout />,
        children: [
          {
            path: "dashboard",
            element: <section>Dashboard</section>,
          },
          {
            path: "*",
            element: <section>Fallback</section>,
          },
        ],
      },
    ],
    {
      initialEntries: ["/dashboard"],
    },
  );

  return render(<RouterProvider router={router} />);
}

function installMatchMedia(initialValue: boolean): MatchMediaController {
  let currentValue = initialValue;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();

  const mediaQueryList = {
    get matches() {
      return currentValue;
    },
    media: "(max-width: 960px)",
    onchange: null,
    addEventListener: (_eventType: string, listener: EventListenerOrEventListenerObject) => {
      if (typeof listener === "function") {
        listeners.add(listener as (event: MediaQueryListEvent) => void);
      }
    },
    removeEventListener: (_eventType: string, listener: EventListenerOrEventListenerObject) => {
      if (typeof listener === "function") {
        listeners.delete(listener as (event: MediaQueryListEvent) => void);
      }
    },
    addListener: (listener: (event: MediaQueryListEvent) => void) => {
      listeners.add(listener);
    },
    removeListener: (listener: (event: MediaQueryListEvent) => void) => {
      listeners.delete(listener);
    },
    dispatchEvent: () => true,
  } as MediaQueryList;

  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockImplementation(() => mediaQueryList),
  });

  return {
    setMatches: (nextValue: boolean) => {
      currentValue = nextValue;
      const event = {
        matches: nextValue,
        media: mediaQueryList.media,
      } as MediaQueryListEvent;

      for (const listener of listeners) {
        listener(event);
      }
    },
  };
}

function getPrimaryNav() {
  const primaryNav = document.getElementById("primary-navigation");
  if (!primaryNav) {
    throw new Error("Expected primary navigation element to exist");
  }

  return primaryNav;
}

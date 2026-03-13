import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const shellPath = resolve(import.meta.dirname, "./shell.tsx");
const routerPath = resolve(import.meta.dirname, "./router.tsx");

describe("accessibility and responsive baselines", () => {
  it("keeps keyboard navigation landmarks and controls in the shell", () => {
    const shellSource = readFileSync(shellPath, "utf8");

    expect(shellSource).toContain('const MAIN_CONTENT_ID = "main-content"');
    expect(shellSource).toContain("id={MAIN_CONTENT_ID}");
    expect(shellSource).toContain("tabIndex={-1}");
    expect(shellSource).toContain('aria-label="Primary"');
    expect(shellSource).toContain('aria-label="Breadcrumb"');
  });

  it("keeps route-level suspense and lazy loading to protect route performance", () => {
    const routerSource = readFileSync(routerPath, "utf8");

    expect(routerSource).toContain("const lazyScreen = (name: ScreenExport) =>");
    expect(routerSource).toContain("lazy(async () => import(\"./screens\")");
    expect(routerSource).toContain("withRouteSuspense(<DashboardPage />)");
    expect(routerSource).toContain("withRouteSuspense(<ReviewHandoffPage />)");
    expect(routerSource).toContain("<Suspense");
    expect(routerSource).toContain('role="status"');
    expect(routerSource).toContain('aria-live="polite"');
  });
});

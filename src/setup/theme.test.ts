// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { followSystemTheme } from "./theme";

afterEach(() => vi.unstubAllGlobals());
describe("安装器外观", () => {
  it("首次及运行中跟随系统深浅色，清理监听且不改应用偏好", () => {
    let callback!: () => void;
    const query = { matches: true, addEventListener: vi.fn((_event, listener) => { callback = listener; }), removeEventListener: vi.fn() };
    vi.stubGlobal("matchMedia", vi.fn(() => query));
    localStorage.setItem("sayall-theme-preference", "light");
    const cleanup = followSystemTheme();
    expect(document.documentElement.dataset.theme).toBe("dark");
    query.matches = false; callback();
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(localStorage.getItem("sayall-theme-preference")).toBe("light");
    cleanup();
    expect(query.removeEventListener).toHaveBeenCalledWith("change", callback);
  });
});

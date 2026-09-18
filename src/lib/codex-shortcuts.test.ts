import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { CODEX_SHORTCUTS, CODEX_SHORTCUT_GROUPS, CODEX_SHORTCUTS_SOURCE, codexShortcutAction } from "./codex-shortcuts";

describe("Codex Windows shortcut catalog", () => {
  it("uses only keys accepted by the Windows backend and valid chord sizes", () => {
    const rust = readFileSync(resolve("crates/sayall-windows/src/send_input.rs"), "utf8");
    const keyEnum = rust.match(/pub enum KeyCode \{([\s\S]+?)\n\}/)![1];
    const keys = new Set(Array.from(keyEnum.matchAll(/^\s+([A-Z][A-Za-z0-9]*),/gm), ([, key]) =>
      key.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase()));
    for (const shortcut of CODEX_SHORTCUTS) {
      expect(shortcut.keys.length, shortcut.id).toBeGreaterThan(0);
      expect(shortcut.keys.length, shortcut.id).toBeLessThanOrEqual(4);
      for (const key of shortcut.keys) expect(keys.has(key), `${shortcut.id}: ${key}`).toBe(true);
    }
    expect(new Set(CODEX_SHORTCUTS.map((item) => item.id)).size).toBe(CODEX_SHORTCUTS.length);
    expect(CODEX_SHORTCUTS.every((item) => CODEX_SHORTCUT_GROUPS.some((group) => group.id === item.group))).toBe(true);
  });

  it("preserves Windows-specific navigation and lookup combinations", () => {
    const keys = (id: string) => CODEX_SHORTCUTS.find((item) => item.id === id)!.keys;
    expect(keys("previous-chat")).toEqual(["control", "page_up"]);
    expect(keys("next-chat")).toEqual(["control", "page_down"]);
    expect(keys("needs-attention")).toEqual(["control", "alt", "a"]);
    expect(keys("find-previous")).toEqual(["shift", "f3"]);
    expect(keys("environment-action")).toEqual(["left_windows", "shift", "d"]);
    expect(keys("keyboard-shortcuts")).toEqual(["control", "slash"]);
    expect(keys("recent-1")).toEqual(["control", "alt", "digit1"]);
    expect(keys("recent-6")).toEqual(["control", "alt", "digit6"]);
    expect(keys("chat-1")).toEqual(["control", "digit1"]);
    expect(keys("chat-9")).toEqual(["control", "digit9"]);
    expect(CODEX_SHORTCUTS_SOURCE).toBe("https://learn.chatgpt.com/docs/reference/commands");
  });

  it("keeps hold-to-dictate out of tap actions and labels contextual approvals", () => {
    expect(CODEX_SHORTCUTS.some((item) => item.keys.join("+") === "control+shift+d")).toBe(false);
    expect(CODEX_SHORTCUTS.find((item) => item.id === "approve")!.note).toContain("审批框");
    expect(CODEX_SHORTCUTS.find((item) => item.id === "approve")!.note).toContain("发送消息");
    expect(CODEX_SHORTCUTS.find((item) => item.id === "decline")!.note).toContain("审批框");
  });

  it("returns independent mapping values without mutating the catalog", () => {
    const shortcut = CODEX_SHORTCUTS.find((item) => item.id === "next-chat")!;
    const action = codexShortcutAction(shortcut);
    expect(action).toEqual({ type: "shortcut", chord: { keys: ["control", "page_down"] } });
    if (action.type !== "shortcut") throw new Error("Expected a shortcut action");
    action.chord.keys.push("shift");
    expect(shortcut.keys).toEqual(["control", "page_down"]);
  });
});

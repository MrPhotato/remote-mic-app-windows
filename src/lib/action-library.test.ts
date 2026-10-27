import { beforeEach, describe, expect, it } from "vitest";
import {
  ACTION_CATALOG, ACTION_LIBRARY_STORAGE_KEY, ACTION_TABS, actionDisplayName, actionEntries,
  actionIdentity, commonActions, emptyActionLibrary, findKnownAction, readActionLibrary,
  recordActionSelection, renameCustomAction, searchActions, setActionPinned, writeActionLibrary,
} from "./action-library";
import type { ButtonAction } from "./bridge";

const chord = (...keys: string[]): ButtonAction => ({ type: "shortcut", chord: { keys } });
const now = 1780000000000;
const DAY = 86400000;

describe("action catalog and labels", () => {
  it("offers five tabs with semantic names and keeps single keys literal", () => {
    expect(ACTION_TABS.map((tab) => tab.label)).toEqual(["常用", "聊天", "编辑", "视图", "系统"]);
    for (const [key, label] of [["escape", "Esc"], ["backspace", "Backspace"], ["enter", "Enter"], ["space", "Space"], ["up", "↑"], ["f11", "F11"]]) {
      expect(actionDisplayName(chord(key))).toBe(label);
    }
    expect(actionDisplayName({ type: "normal_backspace" })).toBe("Backspace");
    expect(actionDisplayName(chord("control", "enter"))).toBe("插队/排队");
    expect(actionDisplayName(chord("control", "alt", "bracket_left"))).toBe("上一个聊天");
    expect(actionDisplayName(chord("control", "shift", "bracket_left"))).toBe("上一个标签页");
    expect(actionDisplayName(chord("control", "alt", "b"))).toBe("切换聊天和标签页");
    expect(new Set(ACTION_CATALOG.map((entry) => entry.id)).size).toBe(ACTION_CATALOG.length);
  });

  it("recognizes modifier ordering and official aliases without mutating actual keys", () => {
    const recorded = chord("bracket_right", "right_shift", "left_control");
    const original = JSON.stringify(recorded);
    expect(findKnownAction(recorded)?.id).toBe("next-tab");
    expect(findKnownAction(chord("bracket_left", "right_alt", "left_control"))?.id).toBe("previous-chat");
    expect(findKnownAction(chord("page_down", "left_control"))).toBeUndefined();
    expect(findKnownAction(chord("b", "left_control"))?.id).toBe("sidebar");
    expect(findKnownAction(chord("right_control", "enter"))?.id).toBe("queue-message");
    expect(actionIdentity(recorded)).toBe(actionIdentity(chord("left_control", "right_shift", "bracket_right")));
    expect(JSON.stringify(recorded)).toBe(original);
    expect(findKnownAction(chord("left_control", "right_control", "enter"))).toBeUndefined();
  });

  it("removes numbered presets without banning custom numbered chords or zoom reset", () => {
    const numbered = ACTION_CATALOG.filter((entry) => entry.action.type === "shortcut" && entry.action.chord.keys.some((key) => /^digit[1-9]$/.test(key)));
    expect(numbered).toEqual([]);
    expect(findKnownAction(chord("control", "digit0"))?.id).toBe("font-reset");
    const state = recordActionSelection(emptyActionLibrary(), chord("control", "alt", "digit7"), "我的切换", now);
    expect(state.custom[0].label).toBe("我的切换");
  });

  it("provides searchable lower-priority mouse and contextual functions", () => {
    expect(searchActions(emptyActionLibrary(), "鼠标").some((entry) => entry.action.type === "mouse_move")).toBe(true);
    expect(searchActions(emptyActionLibrary(), "Ctrl+Enter", "system").map((entry) => entry.id)).toContain("queue-message");
    expect(searchActions(emptyActionLibrary(), "Ctrl+Alt+]").map((entry) => entry.id)).toEqual(["next-chat"]);
    expect(searchActions(emptyActionLibrary(), "Ctrl+Shift+]").map((entry) => entry.id)).toEqual(["next-tab"]);
    expect(searchActions(emptyActionLibrary(), "", "view").map((entry) => entry.id)).toContain("previous-tab");
    expect(searchActions(emptyActionLibrary(), "Ctrl+-").map((entry) => entry.id)).toEqual(["font-decrease"]);
    expect(searchActions(emptyActionLibrary(), "Win+L").map((entry) => entry.id)).toContain("lock-screen");
    expect(findKnownAction(chord("control", "s"))?.label).toBe("保存");
    expect(searchActions(emptyActionLibrary(), "", "edit").findIndex((entry) => entry.more)).toBeGreaterThan(0);
  });
});

describe("learned common actions", () => {
  beforeEach(() => localStorage.clear());

  it("starts from recommendations; reading, searching and pinning do not count a selection", () => {
    const initial = emptyActionLibrary();
    expect(commonActions(initial, now).map((entry) => entry.id)).toContain("queue-message");
    searchActions(initial, "聊天");
    commonActions(initial, now);
    const pinned = setActionPinned(initial, "copy", true);
    expect(pinned.usage).toEqual({});
    expect(commonActions(pinned, now)[0].id).toBe("copy");
    expect(initial.pinned).toEqual([]);
    const afterSave = recordActionSelection(pinned, chord("control", "c"), undefined, now);
    expect(afterSave.usage.copy).toEqual({ count: 1, lastUsed: now });
    expect(initial.usage).toEqual({});
  });

  it("deduplicates an unknown chord across ordering and retains distinct physical modifiers", () => {
    const raw = chord("right_control", "right_alt", "f8");
    const first = recordActionSelection(emptyActionLibrary(), raw, " 我的功能 ", now);
    const id = first.custom[0].id;
    const secondRaw = chord("f8", "right_alt", "right_control");
    const second = recordActionSelection(first, secondRaw, undefined, now + 1);
    expect(second.custom).toHaveLength(1);
    expect(second.custom[0]).toMatchObject({ id, label: "我的功能", action: secondRaw, group: "custom" });
    expect(second.usage[id].count).toBe(2);
    expect(first.custom[0].action).toEqual(raw);
    expect(actionDisplayName(raw, second)).toBe("我的功能");
    if (secondRaw.type === "shortcut") secondRaw.chord.keys.push("shift");
    expect(second.custom[0].action).toEqual(chord("f8", "right_alt", "right_control"));
    const differentModifiers = recordActionSelection(second, chord("left_control", "left_alt", "f8"), "另一个功能", now + 2);
    expect(differentModifiers.custom).toHaveLength(2);
    expect(actionDisplayName(raw, differentModifiers)).toBe("我的功能");
    expect(actionDisplayName(chord("left_control", "left_alt", "f8"), differentModifiers)).toBe("另一个功能");
  });

  it("renames remembered custom shortcuts without assigning or counting again", () => {
    const initial = recordActionSelection(emptyActionLibrary(), chord("alt", "f8"), "旧名称", now);
    const id = initial.custom[0].id;
    const renamed = renameCustomAction(initial, id, "  新名称\n  ");
    expect(renamed.custom[0].label).toBe("新名称");
    expect(renamed.usage).toEqual(initial.usage);
    expect(renameCustomAction(renamed, id, " \n ")).toBe(renamed);
    expect(searchActions(renamed, "新名称")[0].id).toBe(id);
    expect(renameCustomAction(renamed, id, "名".repeat(100)).custom[0].label).toHaveLength(80);
  });

  it("ranks frequency and recency below explicit pins and never deletes old custom entries", () => {
    let state = recordActionSelection(emptyActionLibrary(), chord("alt", "f8"), "较早功能", now - 90 * DAY);
    const oldId = state.custom[0].id;
    state = recordActionSelection(state, chord("control", "v"), undefined, now);
    expect(commonActions(state, now)[0].id).toBe("paste");
    for (let i = 0; i < 24; i++) state = recordActionSelection(state, chord("control", "c"), undefined, now - 10 * DAY);
    expect(commonActions(state, now)[0].id).toBe("copy");
    state = setActionPinned(state, oldId, true);
    expect(commonActions(state, now)[0].id).toBe(oldId);
    expect(commonActions(setActionPinned(state, oldId, false), now)[0].id).toBe("copy");
    expect(commonActions(state, now + 1000 * DAY).some((entry) => entry.id === oldId)).toBe(true);
  });

  it("round-trips names, pins, frequency and exact keys across reloads", () => {
    let state = recordActionSelection(emptyActionLibrary(), chord("right_alt", "f8"), "自定义操作", now);
    state = setActionPinned(state, state.custom[0].id, true);
    expect(writeActionLibrary(state)).toEqual({ ok: true, error: null });
    const reloaded = readActionLibrary();
    expect(reloaded.error).toBeNull();
    expect(reloaded.state).toEqual(state);
    expect(commonActions(reloaded.state, now)[0].label).toBe("自定义操作");
  });

  it("remembers chosen scroll amounts and apps in their category without treating them as custom shortcuts", () => {
    let state = recordActionSelection(emptyActionLibrary(), { type: "scroll", direction: "up", steps: 5 }, undefined, now);
    state = recordActionSelection(state, { type: "open_app", target: "example-app" }, undefined, now);
    expect(state.custom.map((entry) => entry.group)).toEqual(["view", "system"]);
    expect(writeActionLibrary(state).ok).toBe(true);
    expect(readActionLibrary().state).toEqual(state);
    expect(actionEntries(state).filter((entry) => entry.group === "custom")).toEqual([]);
  });

  it("rejects malformed or future data without overwriting the original", () => {
    for (const raw of ["{bad", JSON.stringify({ version: 2, custom: [], usage: {}, pinned: [] }), JSON.stringify({ version: 1, custom: [{ id: "x", group: "custom", label: "x", action: chord("not_a_key") }], usage: {}, pinned: [] })]) {
      localStorage.setItem(ACTION_LIBRARY_STORAGE_KEY, raw);
      expect(readActionLibrary().error).not.toBeNull();
      expect(writeActionLibrary(emptyActionLibrary()).ok).toBe(false);
      expect(localStorage.getItem(ACTION_LIBRARY_STORAGE_KEY)).toBe(raw);
    }
  });

  it("reports inaccessible storage and quota failure, preserving the prior committed library", () => {
    expect(readActionLibrary({ getItem() { throw new Error("blocked"); } }).error).not.toBeNull();
    const original = emptyActionLibrary();
    expect(writeActionLibrary(original).ok).toBe(true);
    const next = recordActionSelection(original, chord("control", "v"), undefined, now);
    const result = writeActionLibrary(next, { getItem: (key) => localStorage.getItem(key), setItem() { throw new DOMException("quota", "QuotaExceededError"); } });
    expect(result.ok).toBe(false);
    expect(result.error).toContain("未能保存");
    expect(readActionLibrary().state).toEqual(original);
  });

  it("keeps catalog recognition and invalid/disabled assignments out of custom entries", () => {
    let state = recordActionSelection(emptyActionLibrary(), chord("left_control", "enter"), "不要覆盖内置名称", now);
    expect(state.custom).toEqual([]);
    expect(state.usage["queue-message"].count).toBe(1);
    state = recordActionSelection(state, { type: "normal_backspace" }, undefined, now);
    expect(state.custom).toEqual([]);
    expect(recordActionSelection(state, { type: "disabled" })).toBe(state);
    expect(recordActionSelection(state, chord("no_such_key"))).toBe(state);
    expect(recordActionSelection(state, { type: "mouse_move", direction: "up", distance: 2001 })).toBe(state);
  });
});

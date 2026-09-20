import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ButtonActions, ButtonMappings } from "./bridge";
import {
  applyCodingProfile, buildCodingProfile, CODING_BACKUP_KEY, CODING_LATEST_BACKUP_KEY, CODING_PROFILE_BUTTONS,
  readCodingBackup, restoreCodingProfile, type CodingProfilePort,
} from "./coding-profile";

const actions = (key: string): ButtonActions => ({
  single: { type: "shortcut", chord: { keys: [key] } },
  double: { type: "open_app", target: "notepad" },
  long: { type: "disabled" },
});

function existing(): ButtonMappings {
  return {
    enabled: false,
    actions: {
      home: actions("home"), ok: actions("tab"), up: actions("page_up"),
      menu: actions("apps"), power: actions("f6"), back: actions("escape"),
      volume_up: actions("volume_up"), volume_down: actions("volume_down"),
      volume_mute: actions("volume_mute"),
    },
    applications: [{ name: "My editor", path: "example-editor.exe" }],
  };
}

function port(current = existing()): CodingProfilePort {
  return {
    loadMappings: vi.fn().mockResolvedValue(current),
    saveMappings: vi.fn(async (value: ButtonMappings) => value),
    storage: localStorage,
    now: () => new Date("2026-09-18T08:00:00Z"),
  };
}

describe("Codex coding profile", () => {
  beforeEach(() => { localStorage.clear(); vi.restoreAllMocks(); });

  it("keeps primary keys immediate, leaves rapid Back presses as deletion, and uses volume keys for navigation", () => {
    const current = existing();
    const original = JSON.stringify(current);
    const next = buildCodingProfile(current);
    expect(next.enabled).toBe(true);
    expect(next.actions.home?.single).toEqual({ type: "open_app", target: "codex" });
    for (const key of ["up", "down", "left", "right"] as const) {
      expect(next.actions[key]?.single).toEqual({ type: "shortcut", chord: { keys: [key] } });
    }
    expect(next.actions.ok?.single).toEqual({ type: "shortcut", chord: { keys: ["enter"] } });
    expect(next.actions.power?.single).toEqual({ type: "shortcut", chord: { keys: ["escape"] } });
    expect(next.actions.back?.single).toEqual({ type: "normal_backspace" });
    expect(next.actions.back?.double).toEqual({ type: "disabled" });
    expect(next.actions.back?.long).toEqual({ type: "disabled" });
    for (const key of ["up", "down", "left", "right", "ok", "power", "volume_up", "volume_down"] as const) {
      expect(next.actions[key]?.double).toEqual({ type: "disabled" });
      expect(next.actions[key]?.long).toEqual({ type: "disabled" });
    }
    expect(next.actions.volume_up?.single).toEqual({ type: "shortcut", chord: { keys: ["control", "page_up"] } });
    expect(next.actions.volume_down?.single).toEqual({ type: "shortcut", chord: { keys: ["control", "page_down"] } });
    expect(CODING_PROFILE_BUTTONS).toHaveLength(12);
    expect(next.actions.volume_mute).toEqual(current.actions.volume_mute);
    expect(next.actions.home?.double).toEqual({ type: "shortcut", chord: { keys: ["control", "n"] } });
    expect(next.actions.home?.long).toEqual({ type: "shortcut", chord: { keys: ["control", "comma"] } });
    expect(next.actions.menu).toEqual({
      single: { type: "shortcut", chord: { keys: ["control", "shift", "p"] } },
      double: { type: "shortcut", chord: { keys: ["control", "shift", "m"] } },
      long: { type: "shortcut", chord: { keys: ["control", "alt", "a"] } },
    });
    expect(next.actions.tv).toEqual({
      single: { type: "shortcut", chord: { keys: ["control", "alt", "b"] } },
      double: { type: "shortcut", chord: { keys: ["control", "b"] } },
      long: { type: "shortcut", chord: { keys: ["control", "z"] } },
    });
    expect(next.applications).toEqual(current.applications);
    expect(JSON.stringify(current)).toBe(original);
    expect(next.actions).not.toHaveProperty("voice");
  });

  it("persists an exact backup before asking the backend to save", async () => {
    const current = existing();
    const deps = port(current);
    deps.saveMappings = vi.fn(async (next) => {
      expect(readCodingBackup(localStorage)?.mappings).toEqual(current);
      expect(readCodingBackup(localStorage, "original")?.mappings).toEqual(current);
      expect(localStorage.getItem(CODING_LATEST_BACKUP_KEY)).not.toBeNull();
      return next;
    });
    const result = await applyCodingProfile(deps);
    expect(result.backup.mappings).toEqual(current);
    expect(deps.saveMappings).toHaveBeenCalledWith(buildCodingProfile(current));
    expect(result.mappings.enabled).toBe(true);
  });

  it("backs up fresh mappings on every apply while preserving the first recovery point", async () => {
    const original = existing();
    await applyCodingProfile(port(original));
    const changed = existing();
    changed.actions.menu = actions("f8");
    changed.applications?.push({ name: "Another editor", path: "another-editor.exe" });
    const result = await applyCodingProfile(port(changed));
    expect(result.mappings.applications).toEqual(changed.applications);
    expect(result.backup.mappings).toEqual(changed);
    expect(readCodingBackup(localStorage, "original")?.mappings).toEqual(original);
    const deps = port(changed);
    expect(await restoreCodingProfile(deps)).toEqual(changed);
    expect(await restoreCodingProfile(deps, "original")).toEqual(original);
  });

  it("restores legacy first backups when a recent snapshot does not exist", async () => {
    const backup = { version: 1, createdAt: "2026-09-18T08:00:00Z", mappings: existing() };
    localStorage.setItem(CODING_BACKUP_KEY, JSON.stringify(backup));
    expect(readCodingBackup(localStorage)).toEqual(backup);
    expect(await restoreCodingProfile(port())).toEqual(existing());
  });

  it("keeps the last custom recovery point when applying the same preset again", async () => {
    await applyCodingProfile(port());
    const backup = localStorage.getItem(CODING_LATEST_BACKUP_KEY);
    const deps = port(buildCodingProfile(existing()));
    const result = await applyCodingProfile(deps);
    expect(result.backup.mappings).toEqual(existing());
    expect(localStorage.getItem(CODING_LATEST_BACKUP_KEY)).toBe(backup);
    expect(deps.saveMappings).not.toHaveBeenCalled();
  });

  it("rejects save failure and retains the recovery point", async () => {
    const deps = port();
    deps.saveMappings = vi.fn().mockRejectedValue(new Error("save failed"));
    await expect(applyCodingProfile(deps)).rejects.toThrow("save failed");
    expect(readCodingBackup(localStorage)?.mappings).toEqual(existing());
  });

  it("never saves when the backup cannot be persisted", async () => {
    const deps = port();
    deps.storage = { getItem: () => null, setItem: () => { throw new Error("quota"); } };
    await expect(applyCodingProfile(deps)).rejects.toThrow("无法保存原有按键备份");
    expect(deps.saveMappings).not.toHaveBeenCalled();
  });

  it("never saves when only the latest snapshot fails to persist", async () => {
    await applyCodingProfile(port());
    const firstBackup = localStorage.getItem(CODING_BACKUP_KEY);
    const deps = port();
    deps.storage = {
      getItem: (key) => localStorage.getItem(key),
      setItem: (key, value) => {
        if (key === CODING_LATEST_BACKUP_KEY) throw new Error("quota");
        localStorage.setItem(key, value);
      },
    };
    await expect(applyCodingProfile(deps)).rejects.toThrow("无法保存原有按键备份");
    expect(deps.saveMappings).not.toHaveBeenCalled();
    expect(localStorage.getItem(CODING_BACKUP_KEY)).toBe(firstBackup);
  });

  it("does not hide or overwrite a corrupt latest snapshot behind a valid original", async () => {
    await applyCodingProfile(port());
    localStorage.setItem(CODING_LATEST_BACKUP_KEY, "corrupt");
    const deps = port();
    await expect(applyCodingProfile(deps)).rejects.toThrow("备份格式不正确");
    await expect(restoreCodingProfile(deps)).rejects.toThrow("备份格式不正确");
    expect(deps.saveMappings).not.toHaveBeenCalled();
    expect(localStorage.getItem(CODING_LATEST_BACKUP_KEY)).toBe("corrupt");
    expect(await restoreCodingProfile(deps, "original")).toEqual(existing());
  });

  it("restores the full original mapping and keeps it available for later recovery", async () => {
    const deps = port();
    await applyCodingProfile(deps);
    const restored = await restoreCodingProfile(deps);
    expect(restored).toEqual(existing());
    expect(deps.saveMappings).toHaveBeenLastCalledWith(existing());
    expect(readCodingBackup(localStorage)?.mappings).toEqual(existing());
  });

  it("retains the original backup if restoring fails", async () => {
    await applyCodingProfile(port());
    const deps = port();
    deps.saveMappings = vi.fn().mockRejectedValue(new Error("restore failed"));
    await expect(restoreCodingProfile(deps)).rejects.toThrow("restore failed");
    expect(readCodingBackup(localStorage)?.mappings).toEqual(existing());
  });

  it.each(["{", "null", JSON.stringify({ version: 1, createdAt: "bad", mappings: existing() }),
    JSON.stringify({ version: 1, createdAt: "2026-09-18", mappings: { enabled: true, actions: { home: {} } } }),
  ])("refuses corrupt backups without overwriting settings (%s)", async (raw) => {
    localStorage.setItem(CODING_BACKUP_KEY, raw);
    const deps = port();
    await expect(restoreCodingProfile(deps)).rejects.toThrow("备份格式不正确");
    await expect(applyCodingProfile(deps)).rejects.toThrow("备份格式不正确");
    expect(deps.saveMappings).not.toHaveBeenCalled();
    expect(localStorage.getItem(CODING_BACKUP_KEY)).toBe(raw);
  });

  it("does not restore when no backup exists", async () => {
    const deps = port();
    await expect(restoreCodingProfile(deps)).rejects.toThrow("尚无可恢复");
    expect(deps.saveMappings).not.toHaveBeenCalled();
  });
});

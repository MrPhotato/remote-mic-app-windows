import type { ButtonAction, ButtonActions, ButtonMappings, RemoteButton } from "./bridge";

export const CODING_BACKUP_KEY = "remoteCoding.codexPresetBackup.v1";
export const CODING_LATEST_BACKUP_KEY = "remoteCoding.codexPresetLatestBackup.v1";
export const CODING_PROFILE_BUTTONS = ["home", "menu", "tv", "power", "up", "down", "left", "right", "ok", "back", "volume_up", "volume_down"] as const;
export type CodingBackupSource = "latest" | "original";

export interface CodingBackup {
  version: 1;
  createdAt: string;
  mappings: ButtonMappings;
}

export interface CodingProfilePort {
  loadMappings: () => Promise<ButtonMappings>;
  saveMappings: (mappings: ButtonMappings) => Promise<ButtonMappings>;
  storage: Pick<Storage, "getItem" | "setItem">;
  now?: () => Date;
}

function copy<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function singleAction(action: ButtonAction): ButtonActions {
  return { single: action, double: { type: "disabled" }, long: { type: "disabled" } };
}

function shortcut(...keys: string[]): ButtonAction {
  return { type: "shortcut", chord: { keys } };
}

/** Voice has a separate setting; primary editing keys retain immediate, repeatable behavior. */
export function buildCodingProfile(current: ButtonMappings): ButtonMappings {
  const next = copy(current);
  next.enabled = true;
  next.actions.home = {
    single: { type: "open_app", target: "codex" },
    double: shortcut("control", "n"),
    long: shortcut("control", "comma"),
  };
  next.actions.menu = {
    single: shortcut("control", "shift", "p"),
    double: shortcut("control", "shift", "m"),
    long: shortcut("control", "alt", "a"),
  };
  next.actions.tv = {
    single: shortcut("control", "b"),
    double: shortcut("control", "alt", "b"),
    long: shortcut("control", "backquote"),
  };
  next.actions.power = singleAction(shortcut("escape"));
  for (const key of ["up", "down", "left", "right"] as const) {
    next.actions[key] = singleAction(shortcut(key));
  }
  next.actions.ok = singleAction(shortcut("enter"));
  next.actions.back = {
    single: { type: "normal_backspace" },
    double: shortcut("control", "z"),
    long: { type: "disabled" },
  };
  next.actions.volume_up = singleAction(shortcut("control", "page_up"));
  next.actions.volume_down = singleAction(shortcut("control", "page_down"));
  return next;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isAction(value: unknown): value is ButtonAction {
  if (!isObject(value)) return false;
  switch (value.type) {
    case "disabled":
    case "normal_backspace":
    case "delete_to_punctuation": return true;
    case "open_app": return typeof value.target === "string" && value.target.length > 0;
    case "shortcut":
      return isObject(value.chord) && Array.isArray(value.chord.keys)
        && value.chord.keys.length > 0 && value.chord.keys.every((key) => typeof key === "string");
    case "scroll":
      return ["up", "down"].includes(String(value.direction))
        && (value.steps === undefined || (Number.isInteger(value.steps) && Number(value.steps) > 0));
    case "mouse_click": return ["left", "right", "double_left", "middle"].includes(String(value.kind));
    case "mouse_move":
      return ["up", "down", "left", "right"].includes(String(value.direction))
        && Number.isInteger(value.distance) && Number(value.distance) > 0;
    default: return false;
  }
}

const KNOWN_BUTTONS: ReadonlySet<string> = new Set<RemoteButton>([
  "home", "up", "down", "left", "right", "ok", "tv", "menu", "power",
  "back", "volume_mute", "volume_up", "volume_down",
]);

function isMappings(value: unknown): value is ButtonMappings {
  return isObject(value) && typeof value.enabled === "boolean" && isObject(value.actions)
    && Object.entries(value.actions).every(([key, actions]) => KNOWN_BUTTONS.has(key)
      && isObject(actions) && [actions.single, actions.double, actions.long].every(isAction))
    && (value.applications === undefined || (Array.isArray(value.applications)
      && value.applications.every((app) => isObject(app)
        && typeof app.name === "string" && typeof app.path === "string")));
}

/** A malformed backup must never become an empty/default configuration on restore. */
export function readCodingBackup(storage: Pick<Storage, "getItem">, source: CodingBackupSource = "latest"): CodingBackup | null {
  let raw: string | null;
  try {
    raw = source === "original" ? storage.getItem(CODING_BACKUP_KEY)
      : storage.getItem(CODING_LATEST_BACKUP_KEY) ?? storage.getItem(CODING_BACKUP_KEY);
  } catch {
    throw new Error("无法读取本地按键备份，请重启应用后重试。");
  }
  if (raw === null) return null;
  try {
    const value: unknown = JSON.parse(raw);
    if (!isObject(value) || value.version !== 1 || typeof value.createdAt !== "string"
      || !Number.isFinite(Date.parse(value.createdAt)) || !isMappings(value.mappings)) {
      throw new Error("invalid backup");
    }
    return value as unknown as CodingBackup;
  } catch {
    throw new Error("本地按键备份格式不正确，已停止应用或恢复预设，现有配置未被覆盖。");
  }
}

export async function applyCodingProfile(port: CodingProfilePort): Promise<{
  mappings: ButtonMappings;
  backup: CodingBackup;
}> {
  // Re-read backend settings so switching pages does not overwrite later customizations.
  const current = await port.loadMappings();
  if (!isMappings(current)) throw new Error("当前按键配置无法备份，已停止应用预设。");
  const original = readCodingBackup(port.storage, "original");
  const recent = readCodingBackup(port.storage); // Stop if an existing recovery point is corrupt.
  const next = buildCodingProfile(current);
  if (recent && JSON.stringify(current) === JSON.stringify(next)) {
    return { mappings: current, backup: recent };
  }
  const backup: CodingBackup = {
    version: 1, createdAt: (port.now?.() ?? new Date()).toISOString(), mappings: copy(current),
  };
  try {
    if (!original) {
      port.storage.setItem(CODING_BACKUP_KEY, JSON.stringify(backup));
    }
    port.storage.setItem(CODING_LATEST_BACKUP_KEY, JSON.stringify(backup));
  } catch {
    throw new Error("无法保存原有按键备份，预设尚未应用。");
  }
  // Preserve the first snapshot, and retain the latest snapshot even if IPC saving fails.
  const mappings = await port.saveMappings(next);
  return { mappings, backup };
}

export async function restoreCodingProfile(port: CodingProfilePort, source: CodingBackupSource = "latest"): Promise<ButtonMappings> {
  const backup = readCodingBackup(port.storage, source);
  if (!backup) throw new Error("尚无可恢复的按键备份。");
  // Keep the backup after restore, so a retry can never lose the recovery point.
  return port.saveMappings(copy(backup.mappings));
}

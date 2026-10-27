import { actionSummary, keyLabel, type ButtonAction, type KeyCode } from "./bridge";
import { CODEX_SHORTCUTS, codexShortcutAction } from "./codex-shortcuts";

export const ACTION_TABS = [
  { id: "common", label: "常用" },
  { id: "chat", label: "聊天" },
  { id: "edit", label: "编辑" },
  { id: "view", label: "视图" },
  { id: "system", label: "系统" },
] as const;
export type ActionTab = typeof ACTION_TABS[number]["id"];
export type ActionGroup = Exclude<ActionTab, "common"> | "custom";
export interface ActionEntry {
  id: string;
  label: string;
  group: ActionGroup;
  action: ButtonAction;
  note?: string;
  more?: boolean;
}

const shortcut = (keys: KeyCode[]): ButtonAction => ({ type: "shortcut", chord: { keys } });
const editIds = new Set(["undo", "redo", "find-chat", "find-next", "find-previous"]);
const systemIds = new Set(["settings", "keyboard-shortcuts", "close", "quit", "fullscreen"]);
const frequentIds = new Set([
  "previous-chat", "next-chat", "previous-tab", "next-tab", "needs-attention", "new-chat", "queue-message", "find-chat",
  "undo", "redo", "review-panel", "sidebar", "bottom-panel", "model-picker", "command-menu",
]);
const shortLabels: Record<string, string> = {
  "standalone-chat": "新建独立聊天", "archive-chat": "归档聊天", "pin-chat": "置顶聊天",
  "rename-chat": "重命名聊天", "side-chat": "打开侧边聊天", "find-chat": "查找聊天内容",
  "copy-link": "复制聊天链接", "file-tree": "开关文件树", "bottom-panel": "开关底部面板",
  terminal: "开关终端", "review-tab": "打开代码审查", "browser-tab": "新建浏览器标签页",
  "focus-address": "定位行或地址栏", "copy-directory": "复制目录或网址", activity: "开关活动视图",
  "font-decrease": "缩小", "font-reset": "恢复缩放", close: "关闭标签页或窗口", quit: "退出应用",
};

export const ACTION_CATALOG: ActionEntry[] = [
  { id: "disabled", label: "未设置", group: "system", action: { type: "disabled" } },
  { id: "open-codex", label: "打开 Codex", group: "system", action: { type: "open_app", target: "codex" } },
  { id: "normal-backspace", label: "Backspace", group: "edit", action: { type: "normal_backspace" }, note: "按住连续删除。" },
  { id: "enter", label: "Enter", group: "edit", action: shortcut(["enter"]), note: "输入框中通常发送，审批框中确认。" },
  { id: "escape", label: "Esc", group: "edit", action: shortcut(["escape"]) },
  ...["up", "down", "left", "right", "tab", "space", "backspace", "delete", "home", "end", "page_up", "page_down", "apps", "f5"].map((key): ActionEntry => ({
    id: `key-${key}`, label: plainKeyName(key), group: "edit", action: shortcut([key]), more: true,
  })),
  ...CODEX_SHORTCUTS.filter((item) => !["approve", "decline", "restore-prompt", "clear-terminal", "copy-session"].includes(item.id)).map((item): ActionEntry => ({
    id: item.id,
    label: shortLabels[item.id] ?? item.label,
    group: editIds.has(item.id) ? "edit" : systemIds.has(item.id) ? "system"
      : ["chats", "navigation", "input"].includes(item.group) ? "chat" : "view",
    action: codexShortcutAction(item),
    note: item.id === "focus-address" ? "取决于当前焦点；在终端中会清屏。" : item.note,
    more: !frequentIds.has(item.id),
  })),
  { id: "copy", label: "复制", group: "edit", action: shortcut(["control", "c"]) },
  { id: "paste", label: "粘贴", group: "edit", action: shortcut(["control", "v"]) },
  { id: "cut", label: "剪切", group: "edit", action: shortcut(["control", "x"]) },
  { id: "select-all", label: "全选", group: "edit", action: shortcut(["control", "a"]) },
  { id: "save", label: "保存", group: "edit", action: shortcut(["control", "s"]), more: true },
  { id: "newline", label: "换行", group: "edit", action: shortcut(["shift", "enter"]) },
  { id: "delete-to-punctuation", label: "删到上一个标点", group: "edit", action: { type: "delete_to_punctuation" }, note: "保留标点；取决于输入框的辅助功能支持。", more: true },
  { id: "scroll-up", label: "向上滚动", group: "view", action: { type: "scroll", direction: "up", steps: 3 } },
  { id: "scroll-down", label: "向下滚动", group: "view", action: { type: "scroll", direction: "down", steps: 3 } },
  { id: "switch-window", label: "切换窗口", group: "system", action: shortcut(["alt", "tab"]) },
  { id: "show-desktop", label: "显示桌面", group: "system", action: shortcut(["left_windows", "d"]), more: true },
  { id: "lock-screen", label: "锁定电脑", group: "system", action: shortcut(["left_windows", "l"]), more: true },
  { id: "system-search", label: "搜索", group: "system", action: shortcut(["left_windows", "s"]), more: true },
  { id: "screenshot", label: "截图", group: "system", action: shortcut(["left_windows", "shift", "s"]), more: true },
  ...["volume_up", "volume_down", "volume_mute", "media_play_pause", "media_prev", "media_next"].map((key): ActionEntry => ({
    id: `key-${key}`, label: plainKeyName(key), group: "system", action: shortcut([key]), more: true,
  })),
  ...(["left", "right", "double_left", "middle"] as const).map((kind): ActionEntry => ({
    id: `mouse-${kind}`, label: actionSummary({ type: "mouse_click", kind }), group: "system", action: { type: "mouse_click", kind }, more: true,
  })),
  ...(["up", "down", "left", "right"] as const).map((direction): ActionEntry => ({
    id: `mouse-move-${direction}`, label: actionSummary({ type: "mouse_move", direction, distance: 40 }), group: "system", action: { type: "mouse_move", direction, distance: 40 }, more: true,
  })),
];

const MODIFIER_ALIASES: Record<string, string> = {
  left_control: "control", right_control: "control", left_shift: "shift", right_shift: "shift",
  left_alt: "alt", right_alt: "alt", left_windows: "windows", right_windows: "windows",
};

/** Stable exact identity: key order is irrelevant, physical left/right modifiers are not. */
export function actionIdentity(action: ButtonAction): string {
  switch (action.type) {
    case "shortcut": return `shortcut:${[...action.chord.keys].sort().join("+")}`;
    case "scroll": return `scroll:${action.direction}:${action.steps ?? 1}`;
    case "mouse_click": return `mouse_click:${action.kind}`;
    case "mouse_move": return `mouse_move:${action.direction}:${action.distance}`;
    case "open_app": return `open_app:${action.target}`;
    default: return action.type;
  }
}

/** Only built-in recognition treats left/right modifier variants as the same named command. */
function recognitionIdentity(action: ButtonAction): string {
  return action.type === "shortcut"
    ? `shortcut:${action.chord.keys.map((key) => MODIFIER_ALIASES[key] ?? key).sort().join("+")}`
    : actionIdentity(action);
}

export function findKnownAction(action: ButtonAction): ActionEntry | undefined {
  const identity = recognitionIdentity(action);
  const direct = ACTION_CATALOG.find((entry) => recognitionIdentity(entry.action) === identity);
  if (direct) return direct;
  const alias = CODEX_SHORTCUTS.find((entry) => entry.aliases?.some((keys) => recognitionIdentity(shortcut(keys)) === identity));
  return alias ? ACTION_CATALOG.find((entry) => entry.id === alias.id) : undefined;
}

function plainKeyName(key: KeyCode): string {
  return ({ backspace: "Backspace", space: "Space", home: "Home", apps: "Menu", windows: "Win" } as Record<string, string>)[key] ?? keyLabel(key);
}

export function actionDisplayName(action: ButtonAction | undefined, library?: ActionLibraryState): string {
  if (!action || action.type === "disabled") return "未设置";
  if (action.type === "normal_backspace") return "Backspace";
  // Single keys always keep their key names, even when a user once named the entry.
  if (action.type === "shortcut" && action.chord.keys.length === 1) return plainKeyName(action.chord.keys[0]);
  const known = findKnownAction(action);
  if (known) return known.label;
  const custom = library?.custom.find((entry) => actionIdentity(entry.action) === actionIdentity(action));
  return custom?.label ?? actionSummary(action);
}

export interface ActionLibraryState {
  version: 1;
  custom: ActionEntry[];
  usage: Record<string, { count: number; lastUsed: number }>;
  pinned: string[];
}
export const ACTION_LIBRARY_STORAGE_KEY = "sayall-action-library-v1";
export function emptyActionLibrary(): ActionLibraryState {
  return { version: 1, custom: [], usage: {}, pinned: [] };
}

const namedKeys = new Set([
  "control", "left_control", "right_control", "shift", "left_shift", "right_shift", "alt", "left_alt", "right_alt",
  "left_windows", "right_windows", "backspace", "tab", "enter", "escape", "space", "page_up", "page_down", "end", "home",
  "left", "up", "right", "down", "insert", "delete", "apps", "volume_mute", "volume_down", "volume_up",
  "media_prev", "media_next", "media_play_pause", "comma", "slash", "bracket_left", "bracket_right", "backquote", "equal", "minus", "period",
]);
const validKey = (key: unknown): key is string => typeof key === "string" && (namedKeys.has(key) || /^(?:[a-z]|digit[0-9]|f(?:[1-9]|1[0-2]))$/.test(key));
const object = (value: unknown): value is Record<string, unknown> => !!value && typeof value === "object" && !Array.isArray(value);
const positiveInteger = (value: unknown, max: number): value is number => Number.isInteger(value) && Number(value) > 0 && Number(value) <= max;

function validAction(value: unknown): value is ButtonAction {
  if (!object(value)) return false;
  switch (value.type) {
    case "disabled": case "normal_backspace": case "delete_to_punctuation": return true;
    case "shortcut": return object(value.chord) && Array.isArray(value.chord.keys) && value.chord.keys.length > 0 && value.chord.keys.length <= 4 && value.chord.keys.every(validKey) && new Set(value.chord.keys).size === value.chord.keys.length;
    case "scroll": return ["up", "down"].includes(String(value.direction)) && (value.steps === undefined || positiveInteger(value.steps, 100));
    case "mouse_click": return ["left", "right", "double_left", "middle"].includes(String(value.kind));
    case "mouse_move": return ["up", "down", "left", "right"].includes(String(value.direction)) && positiveInteger(value.distance, 2000);
    case "open_app": return typeof value.target === "string" && value.target.length > 0 && value.target.length <= 32768;
    default: return false;
  }
}

function cloneAction(action: ButtonAction): ButtonAction {
  return action.type === "shortcut" ? { type: "shortcut", chord: { keys: [...action.chord.keys] } } : { ...action };
}
function cleanLabel(label: string): string {
  return label.replace(/[\u0000-\u001f\u007f]/g, " ").trim().slice(0, 80);
}
function customId(action: ButtonAction): string { return `custom:${actionIdentity(action)}`; }
function customGroup(action: ButtonAction): ActionGroup { return action.type === "shortcut" ? "custom" : action.type === "scroll" ? "view" : "system"; }

function parseLibrary(raw: string): ActionLibraryState {
  const value: unknown = JSON.parse(raw);
  if (!object(value) || value.version !== 1 || !Array.isArray(value.custom) || !object(value.usage) || !Array.isArray(value.pinned)) throw new Error("schema");
  const state = emptyActionLibrary();
  for (const entry of value.custom) {
    if (!object(entry) || typeof entry.label !== "string" || !cleanLabel(entry.label) || entry.label.length > 80 || !validAction(entry.action) || entry.group !== customGroup(entry.action) || entry.id !== customId(entry.action)) throw new Error("custom");
    if (state.custom.some((existing) => existing.id === entry.id)) throw new Error("duplicate");
    state.custom.push({ id: entry.id, label: cleanLabel(entry.label), group: customGroup(entry.action), action: cloneAction(entry.action) });
  }
  for (const [id, usage] of Object.entries(value.usage)) {
    if (!object(usage) || !positiveInteger(usage.count, Number.MAX_SAFE_INTEGER) || !Number.isSafeInteger(usage.lastUsed) || Number(usage.lastUsed) < 0) throw new Error("usage");
    Object.defineProperty(state.usage, id, { value: { count: usage.count, lastUsed: usage.lastUsed }, enumerable: true, writable: true, configurable: true });
  }
  if (!value.pinned.every((id) => typeof id === "string") || new Set(value.pinned).size !== value.pinned.length) throw new Error("pins");
  state.pinned = [...value.pinned] as string[];
  return state;
}

export function readActionLibrary(storage?: Pick<Storage, "getItem">): { state: ActionLibraryState; error: string | null } {
  try {
    const raw = (storage ?? localStorage).getItem(ACTION_LIBRARY_STORAGE_KEY);
    return { state: raw === null ? emptyActionLibrary() : parseLibrary(raw), error: null };
  } catch {
    return { state: emptyActionLibrary(), error: "常用记录暂时无法读取，原记录已保留。" };
  }
}

/** A failed write leaves both persisted data and the caller's current state untouched. */
export function writeActionLibrary(state: ActionLibraryState, storage?: Pick<Storage, "getItem" | "setItem">): { ok: boolean; error: string | null } {
  try {
    const target = storage ?? localStorage;
    const existing = target.getItem(ACTION_LIBRARY_STORAGE_KEY);
    if (existing !== null) parseLibrary(existing); // Never overwrite an unreadable or newer-format library.
    const encoded = JSON.stringify(state);
    parseLibrary(encoded);
    target.setItem(ACTION_LIBRARY_STORAGE_KEY, encoded);
    return { ok: true, error: null };
  } catch {
    return { ok: false, error: "常用记录未能保存，已有记录未改动。" };
  }
}

/** Call only after an assignment was successfully saved, never on click, capture or remote key use. */
export function recordActionSelection(state: ActionLibraryState, action: ButtonAction, customLabel?: string, now = Date.now()): ActionLibraryState {
  if (!validAction(action) || action.type === "disabled") return state;
  const known = findKnownAction(action);
  const id = known?.id ?? customId(action);
  const custom = state.custom.map((entry) => ({ ...entry, action: cloneAction(entry.action) }));
  if (!known) {
    const existing = custom.find((entry) => entry.id === id);
    const label = cleanLabel(customLabel ?? "") || existing?.label || cleanLabel(actionDisplayName(action));
    const entry: ActionEntry = { id, label, group: customGroup(action), action: cloneAction(action) };
    if (existing) custom[custom.indexOf(existing)] = entry;
    else custom.push(entry);
  }
  const previous = state.usage[id];
  return {
    version: 1, custom, pinned: [...state.pinned],
    usage: { ...state.usage, [id]: { count: Math.min((previous?.count ?? 0) + 1, Number.MAX_SAFE_INTEGER), lastUsed: Math.max(0, Math.floor(now)) } },
  };
}

export function renameCustomAction(state: ActionLibraryState, id: string, label: string): ActionLibraryState {
  const cleaned = cleanLabel(label);
  if (!cleaned || !state.custom.some((entry) => entry.id === id)) return state;
  return { ...state, custom: state.custom.map((entry) => entry.id === id ? { ...entry, label: cleaned } : entry) };
}

export function setActionPinned(state: ActionLibraryState, id: string, pinned: boolean): ActionLibraryState {
  if (!actionEntries(state).some((entry) => entry.id === id)) return state;
  return { ...state, pinned: pinned ? [...new Set([...state.pinned, id])] : state.pinned.filter((existing) => existing !== id) };
}

export function actionEntries(state: ActionLibraryState): ActionEntry[] {
  // A future built-in definition may recognize an old custom entry. Preserve it on disk but do not duplicate it in the picker.
  return [...ACTION_CATALOG, ...state.custom.filter((entry) => !findKnownAction(entry.action))];
}

const DEFAULT_COMMON_IDS = ["previous-chat", "next-chat", "queue-message", "open-codex", "new-chat", "needs-attention", "enter", "normal-backspace", "undo", "redo", "review-panel", "sidebar"];
export function commonActions(state: ActionLibraryState, now = Date.now()): ActionEntry[] {
  const all = actionEntries(state);
  const score = (entry: ActionEntry): number => {
    const usage = state.usage[entry.id];
    if (!usage) return 0;
    const days = Math.max(0, now - usage.lastUsed) / 86400000;
    return Math.log2(usage.count + 1) + 3 / (1 + days / 7);
  };
  const learned = all.filter((entry) => state.usage[entry.id]).sort((a, b) => score(b) - score(a) || a.id.localeCompare(b.id));
  const orderedIds = [...state.pinned, ...learned.map((entry) => entry.id), ...DEFAULT_COMMON_IDS, ...state.custom.map((entry) => entry.id)];
  const seen = new Set<string>();
  return orderedIds.flatMap((id) => {
    const entry = all.find((candidate) => candidate.id === id);
    if (!entry || seen.has(id)) return [];
    seen.add(id);
    return [entry];
  });
}

function searchText(value: string): string { return value.toLowerCase().replace(/control/g, "ctrl").replace(/[\s＋+_]/g, ""); }
export function searchActions(state: ActionLibraryState, query: string, tab: ActionTab = "common", now = Date.now()): ActionEntry[] {
  const normalized = searchText(query);
  const entries = normalized ? actionEntries(state) : tab === "common" ? commonActions(state, now) : actionEntries(state).filter((entry) => entry.group === tab);
  const sorted = tab === "common" && !normalized ? entries : [...entries].sort((a, b) => Number(!!a.more) - Number(!!b.more));
  if (!normalized) return sorted;
  return sorted.filter((entry) => {
    const keys = entry.action.type === "shortcut" ? entry.action.chord.keys.map((key) => plainKeyName(MODIFIER_ALIASES[key] ?? key)).join("+") : "";
    const aliases = CODEX_SHORTCUTS.find((shortcut) => shortcut.id === entry.id)?.aliases?.map((keys) => keys.map(plainKeyName).join("+")).join(" ") ?? "";
    return searchText(`${entry.label} ${entry.note ?? ""} ${keys} ${aliases}`).includes(normalized);
  });
}

import type { ButtonAction, KeyCode } from "./bridge";

/** Public Windows defaults, checked against the official command reference. */
export const CODEX_SHORTCUTS_SOURCE = "https://learn.chatgpt.com/docs/reference/commands";
export const CODEX_SHORTCUTS_VERIFIED = "2026-09-18";

export const CODEX_SHORTCUT_GROUPS = [
  { id: "navigation", label: "切换会话" },
  { id: "chats", label: "管理会话" },
  { id: "workspace", label: "工作区与浏览器" },
  { id: "input", label: "输入与审批" },
  { id: "window", label: "窗口与设置" },
] as const;

export type CodexShortcutGroup = typeof CODEX_SHORTCUT_GROUPS[number]["id"];
export interface CodexShortcut {
  id: string;
  group: CodexShortcutGroup;
  label: string;
  keys: KeyCode[];
  note?: string;
}

export const CODEX_SHORTCUTS: CodexShortcut[] = [
  { id: "previous-chat", group: "navigation", label: "上一个会话或标签页", keys: ["control", "page_up"], note: "在会话与打开的标签页之间切换。" },
  { id: "next-chat", group: "navigation", label: "下一个会话或标签页", keys: ["control", "page_down"], note: "在会话与打开的标签页之间切换。" },
  { id: "needs-attention", group: "navigation", label: "下一个待处理会话", keys: ["control", "alt", "a"], note: "切换到需要你处理的 Codex 会话。" },
  { id: "navigate-back", group: "navigation", label: "返回上一视图", keys: ["control", "bracket_left"] },
  { id: "navigate-forward", group: "navigation", label: "前往下一视图", keys: ["control", "bracket_right"] },
  ...Array.from({ length: 6 }, (_, index): CodexShortcut => ({ id: `recent-${index + 1}`, group: "navigation", label: `最近会话 ${index + 1}`, keys: ["control", "alt", `digit${index + 1}`] })),
  ...Array.from({ length: 9 }, (_, index): CodexShortcut => ({ id: `chat-${index + 1}`, group: "navigation", label: `跳到会话 ${index + 1}`, keys: ["control", `digit${index + 1}`] })),
  { id: "new-chat", group: "chats", label: "新建会话", keys: ["control", "n"] },
  { id: "standalone-chat", group: "chats", label: "新建独立会话", keys: ["control", "alt", "o"], note: "Codex 独立会话。" },
  { id: "archive-chat", group: "chats", label: "归档当前会话", keys: ["control", "shift", "a"], note: "会将当前会话归档。" },
  { id: "unread-chat", group: "chats", label: "标为未读", keys: ["control", "shift", "u"] },
  { id: "pin-chat", group: "chats", label: "置顶或取消置顶", keys: ["control", "alt", "p"] },
  { id: "rename-chat", group: "chats", label: "重命名会话", keys: ["control", "alt", "r"] },
  { id: "side-chat", group: "chats", label: "打开侧边会话", keys: ["control", "alt", "s"] },
  { id: "find-chat", group: "chats", label: "查找当前会话", keys: ["control", "f"] },
  { id: "find-next", group: "chats", label: "下一个匹配项", keys: ["control", "g"], note: "已打开会话内查找时使用。" },
  { id: "find-previous", group: "chats", label: "上一个匹配项", keys: ["shift", "f3"], note: "已打开会话内查找时使用。" },
  { id: "clear-unread", group: "chats", label: "清除全部未读标记", keys: ["shift", "escape"] },
  { id: "copy-link", group: "chats", label: "复制会话链接", keys: ["control", "alt", "l"] },
  { id: "copy-session", group: "chats", label: "复制会话 ID", keys: ["control", "alt", "c"] },
  { id: "open-folder", group: "workspace", label: "打开文件夹", keys: ["control", "o"] },
  { id: "search-files", group: "workspace", label: "搜索文件", keys: ["control", "p"] },
  { id: "file-tree", group: "workspace", label: "显示或隐藏文件树", keys: ["control", "shift", "e"] },
  { id: "bottom-panel", group: "workspace", label: "显示或隐藏底部面板", keys: ["control", "j"] },
  { id: "terminal", group: "workspace", label: "显示或隐藏终端", keys: ["control", "backquote"] },
  { id: "clear-terminal", group: "workspace", label: "清空终端显示", keys: ["control", "l"], note: "仅在终端获得焦点时生效；其他视图中可能定位行或地址栏。" },
  { id: "environment-action", group: "workspace", label: "运行环境主操作", keys: ["left_windows", "shift", "d"], note: "需要当前环境已定义主操作；按下会执行该操作。" },
  { id: "review-tab", group: "workspace", label: "打开代码审查标签页", keys: ["control", "shift", "g"] },
  { id: "review-panel", group: "workspace", label: "显示或隐藏审查面板", keys: ["control", "alt", "b"] },
  { id: "browser-tab", group: "workspace", label: "打开浏览器标签页", keys: ["control", "t"], note: "需要内置浏览器功能可用。" },
  { id: "browser-panel", group: "workspace", label: "显示或隐藏浏览器", keys: ["control", "shift", "b"], note: "需要内置浏览器功能可用。" },
  { id: "focus-address", group: "workspace", label: "定位行或浏览器地址栏", keys: ["control", "l"], note: "取决于当前焦点；终端内会清屏。" },
  { id: "browser-back", group: "workspace", label: "浏览器后退", keys: ["alt", "left"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-forward", group: "workspace", label: "浏览器前进", keys: ["alt", "right"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-reload", group: "workspace", label: "刷新网页", keys: ["control", "r"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-reload-full", group: "workspace", label: "忽略缓存刷新网页", keys: ["control", "shift", "r"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "copy-directory", group: "workspace", label: "复制工作目录或网页地址", keys: ["control", "shift", "c"], note: "浏览器获得焦点时复制网页地址，否则复制工作目录。" },
  { id: "browser-comment", group: "workspace", label: "切换浏览与批注", keys: ["control", "period"], note: "需要浏览器批注功能可用。" },
  { id: "model-picker", group: "input", label: "选择模型", keys: ["control", "shift", "m"] },
  { id: "project-picker", group: "input", label: "选择项目", keys: ["control", "alt", "shift", "o"] },
  { id: "voice-chat", group: "input", label: "开始语音聊天", keys: ["control", "shift", "v"], note: "需要语音聊天功能可用；与按住听写不同。" },
  { id: "restore-prompt", group: "input", label: "恢复上一条输入", keys: ["up"], note: "仅在输入框为空且获得焦点时生效。" },
  { id: "approve", group: "input", label: "批准当前请求", keys: ["enter"], note: "仅限审批框已打开时；输入框内 Enter 可能发送消息。" },
  { id: "decline", group: "input", label: "拒绝当前请求", keys: ["escape"], note: "仅限审批框已打开时；其他位置会执行普通 Esc。" },
  { id: "activity", group: "input", label: "显示或隐藏活动视图", keys: ["control", "alt", "u"], note: "需要活动视图功能可用。" },
  ...Array.from({ length: 3 }, (_, index): CodexShortcut => ({ id: `mode-${index + 1}`, group: "input", label: `切换到模式 ${index + 1}`, keys: ["alt", `digit${index + 1}`], note: "按应用显示的 Chat、Work、Codex 模式顺序切换。" })),
  { id: "command-menu", group: "window", label: "打开命令菜单", keys: ["control", "shift", "p"] },
  { id: "settings", group: "window", label: "打开设置", keys: ["control", "comma"] },
  { id: "keyboard-shortcuts", group: "window", label: "查看快捷键", keys: ["control", "slash"] },
  { id: "font-increase", group: "window", label: "放大字体", keys: ["control", "equal"] },
  { id: "font-decrease", group: "window", label: "缩小字体", keys: ["control", "minus"] },
  { id: "font-reset", group: "window", label: "恢复默认字号", keys: ["control", "digit0"] },
  { id: "sidebar", group: "window", label: "显示或隐藏侧栏", keys: ["control", "b"] },
  { id: "undo", group: "window", label: "撤销上一步操作", keys: ["control", "z"], note: "根据焦点撤销输入或最近的应用操作。" },
  { id: "redo", group: "window", label: "重做上一步操作", keys: ["control", "shift", "z"] },
  { id: "close", group: "window", label: "关闭当前标签页或窗口", keys: ["control", "w"] },
  { id: "fullscreen", group: "window", label: "切换全屏", keys: ["f11"] },
  { id: "quit", group: "window", label: "退出 Codex", keys: ["control", "q"], note: "会退出整个应用。" },
];

export function codexShortcutAction(shortcut: CodexShortcut): ButtonAction {
  return { type: "shortcut", chord: { keys: [...shortcut.keys] } };
}

import type { ButtonAction, KeyCode } from "./bridge";

/** Public Windows defaults, checked against the official command reference. */
export const CODEX_SHORTCUTS_SOURCE = "https://learn.chatgpt.com/docs/reference/commands";
export const CODEX_SHORTCUTS_VERIFIED = "2026-10-14";

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
  aliases?: KeyCode[][];
  note?: string;
}

export const CODEX_SHORTCUTS: CodexShortcut[] = [
  { id: "previous-chat", group: "navigation", label: "上一个聊天", keys: ["control", "shift", "bracket_left"], aliases: [["control", "page_up"], ["control", "shift", "tab"]], note: "在聊天与打开的标签页之间切换。" },
  { id: "next-chat", group: "navigation", label: "下一个聊天", keys: ["control", "shift", "bracket_right"], aliases: [["control", "page_down"], ["control", "tab"]], note: "在聊天与打开的标签页之间切换。" },
  { id: "needs-attention", group: "navigation", label: "处理待办聊天", keys: ["control", "alt", "a"], note: "Codex 在前台时，切换到需要你处理的聊天。" },
  { id: "navigate-back", group: "navigation", label: "返回", keys: ["control", "bracket_left"] },
  { id: "navigate-forward", group: "navigation", label: "前进", keys: ["control", "bracket_right"] },
  { id: "new-chat", group: "chats", label: "新建聊天", keys: ["control", "n"], aliases: [["control", "shift", "o"]] },
  { id: "queue-message", group: "chats", label: "插队/排队", keys: ["control", "enter"], note: "在输入框中使用，具体行为取决于当前任务状态。" },
  { id: "temporary-chat", group: "chats", label: "新建临时聊天", keys: ["control", "shift", "n"], note: "ChatGPT 临时聊天。" },
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
  { id: "review-panel", group: "workspace", label: "切换聊天和标签页", keys: ["control", "alt", "b"], note: "分屏时显示或隐藏标签页。" },
  { id: "browser-tab", group: "workspace", label: "打开浏览器标签页", keys: ["control", "t"], note: "需要内置浏览器功能可用。" },
  { id: "browser-panel", group: "workspace", label: "切换工作区布局", keys: ["control", "shift", "b"], note: "在全屏内容、分屏和隐藏标签页之间切换。" },
  { id: "focus-address", group: "workspace", label: "定位行或浏览器地址栏", keys: ["control", "l"], note: "取决于当前焦点；终端内会清屏。" },
  { id: "browser-back", group: "workspace", label: "浏览器后退", keys: ["alt", "left"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-forward", group: "workspace", label: "浏览器前进", keys: ["alt", "right"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-reload", group: "workspace", label: "刷新网页", keys: ["control", "r"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "browser-reload-full", group: "workspace", label: "忽略缓存刷新网页", keys: ["control", "shift", "r"], note: "仅在内置浏览器获得焦点时生效。" },
  { id: "copy-directory", group: "workspace", label: "复制工作目录或网页地址", keys: ["control", "shift", "c"], note: "浏览器获得焦点时复制网页地址，否则复制工作目录。" },
  { id: "browser-comment", group: "workspace", label: "切换浏览与批注", keys: ["control", "shift", "period"], note: "需要浏览器批注功能可用。" },
  { id: "model-picker", group: "input", label: "选择模型", keys: ["control", "shift", "m"] },
  { id: "project-picker", group: "input", label: "选择项目", keys: ["control", "alt", "shift", "o"] },
  { id: "restore-prompt", group: "input", label: "↑", keys: ["up"], note: "输入框为空时可恢复上一条输入。" },
  { id: "approve", group: "input", label: "Enter", keys: ["enter"], note: "审批框中确认；输入框内可能发送消息。" },
  { id: "decline", group: "input", label: "Esc", keys: ["escape"], note: "审批框中拒绝；其他位置执行普通 Esc。" },
  { id: "activity", group: "input", label: "显示或隐藏活动视图", keys: ["control", "alt", "u"], note: "需要活动视图功能可用。" },
  { id: "command-menu", group: "window", label: "打开命令菜单", keys: ["control", "shift", "p"] },
  { id: "settings", group: "window", label: "打开设置", keys: ["control", "comma"] },
  { id: "keyboard-shortcuts", group: "window", label: "查看快捷键", keys: ["control", "slash"] },
  { id: "font-increase", group: "window", label: "放大", keys: ["control", "shift", "equal"], aliases: [["control", "equal"]] },
  { id: "font-decrease", group: "window", label: "缩小字体", keys: ["control", "minus"] },
  { id: "font-reset", group: "window", label: "恢复默认字号", keys: ["control", "digit0"] },
  { id: "sidebar", group: "window", label: "开关侧边栏", keys: ["control", "shift", "s"], aliases: [["control", "b"]] },
  { id: "undo", group: "window", label: "撤销", keys: ["control", "z"], note: "根据焦点撤销输入或最近的应用操作。" },
  { id: "redo", group: "window", label: "重做", keys: ["control", "y"], aliases: [["control", "shift", "z"]] },
  { id: "close", group: "window", label: "关闭当前标签页或窗口", keys: ["control", "w"] },
  { id: "fullscreen", group: "window", label: "F11", keys: ["f11"], note: "切换全屏。" },
  { id: "quit", group: "window", label: "退出 Codex", keys: ["control", "q"], note: "会退出整个应用。" },
];

export function codexShortcutAction(shortcut: CodexShortcut): ButtonAction {
  return { type: "shortcut", chord: { keys: [...shortcut.keys] } };
}

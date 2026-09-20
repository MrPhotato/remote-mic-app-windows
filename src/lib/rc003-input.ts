import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./bridge";
import { reportFrontendEvent } from "./frontend-diagnostics";

export interface Rc003InputStatus {
  phase: "stopped" | "starting" | "waiting" | "ready" | "failed";
  lastError: string | null;
  generation: number;
  reportCount: number;
  edgeCount: number;
}

export function stoppedRc003InputStatus(): Rc003InputStatus {
  return { phase: "stopped", lastError: null, generation: 0, reportCount: 0, edgeCount: 0 };
}

export function rc003InputAvailable(): boolean {
  return isTauriRuntime() && import.meta.env.VITE_SAYALL_RUNTIME_SIMULATION !== "1";
}

export function getRc003InputStatus(): Promise<Rc003InputStatus> {
  if (!rc003InputAvailable()) return Promise.resolve(stoppedRc003InputStatus());
  return invoke<Rc003InputStatus>("get_rc003_input_status");
}

// Coalesce clicks across page changes while Windows is showing the authorization dialog.
let pendingChange: { command: string; promise: Promise<Rc003InputStatus> } | null = null;

function changeRc003Input(command: "start_rc003_input" | "stop_rc003_input"): Promise<Rc003InputStatus> {
  if (!rc003InputAvailable()) return Promise.reject(new Error("preview_unavailable"));
  if (pendingChange) {
    return pendingChange.command === command
      ? pendingChange.promise
      : Promise.reject(new Error("operation_pending"));
  }
  const event = command === "start_rc003_input" ? "rc003_input_start" : "rc003_input_stop";
  const started = performance.now();
  reportFrontendEvent({ event, phase: "started", result: "unknown", reason: "user_requested" });
  const promise = invoke<Rc003InputStatus>(command).then((status) => {
    reportFrontendEvent({ event, phase: "completed", result: status.phase === "failed" ? "failed" : "passed",
      reason: `phase_${status.phase}`, elapsedMs: Math.round(performance.now() - started) });
    return status;
  }, (cause: unknown) => {
    reportFrontendEvent({ event, phase: "completed", result: "failed", reason: "command_failed",
      elapsedMs: Math.round(performance.now() - started) });
    throw cause;
  }).finally(() => { pendingChange = null; });
  pendingChange = { command, promise };
  return promise;
}

export function startRc003Input(): Promise<Rc003InputStatus> {
  return changeRc003Input("start_rc003_input");
}

export function stopRc003Input(): Promise<Rc003InputStatus> {
  return changeRc003Input("stop_rc003_input");
}

/** Translate failures without exposing a helper's paths, process details or device identity. */
export function rc003InputErrorMessage(cause: unknown): string {
  const code = (cause instanceof Error ? cause.message : String(cause ?? "")).toLowerCase();
  const knownErrors: Record<string, string> = {
    "请先连接 rc003 遥控器并启动按键监听。": "请先在「连接与语音」中连接 RC003，并在「按键映射」中启动按键监听，然后再启用三键增强。",
    helper_start_failed: "无法启动三键增强，请重试；若仍失败，可查看诊断日志。",
    helper_start_timeout: "三键增强启动超时，请重新启用；若仍失败，可查看诊断日志。",
    helper_heartbeat_timeout: "三键增强响应超时，请重新启用；若仍失败，可查看诊断日志。",
    helper_exited: "三键增强已退出，请重新启用。",
    helper_disconnected: "三键增强连接已中断，请重新启用。",
    ipc_authentication_failed: "三键增强安全校验未通过，请重试；若仍失败，可查看诊断日志。",
    ipc_setup_failed: "无法建立三键增强通信，请重试；若仍失败，可查看诊断日志。",
  };
  if (knownErrors[code]) return knownErrors[code];
  if (/cancel|denied|elevation|permission|1223|授权|权限/.test(code)) {
    return "未获得 Windows 管理员授权。请再次点击启用，并在授权窗口中允许。";
  }
  if (/preview|simulation|unsupported|not_supported/.test(code)) {
    return "请在 Windows 客户端中使用三键增强；浏览器预览和仿真模式无法启用。";
  }
  if (/missing|not_found|not found|dependency|未找到|缺少/.test(code)) {
    return "三键增强组件不完整，请安装包含三键增强组件的完整版本后重试。";
  }
  if (/operation_pending|already_starting/.test(code)) {
    return "上一项操作尚未完成，请先处理 Windows 授权窗口。";
  }
  if (/timeout|timed out|超时/.test(code)) {
    return "三键增强启动超时，请重试；若仍失败，可查看诊断日志。";
  }
  return "三键增强未能完成操作，请重试；若仍失败，可查看诊断日志。";
}

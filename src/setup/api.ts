import { invoke, isTauri } from "@tauri-apps/api/core";

export interface SetupInfo { version: string; directory: string; existing: boolean }
export interface InstalledInfo { directory: string; version: string }
export const preview = !isTauri();

export const setupApi = {
  info: (): Promise<SetupInfo> => preview
    ? Promise.resolve({ version: "预览", directory: "C:\\Apps\\SayAll", existing: false })
    : invoke("setup_info"),
  browse: (directory: string): Promise<string | null> => invoke("setup_browse_directory", { directory }),
  install: (directory: string, desktopShortcut: boolean): Promise<InstalledInfo> =>
    invoke("setup_install", { directory, desktopShortcut }),
  finish: (launch: boolean): Promise<void> => invoke("setup_finish", { launch }),
};

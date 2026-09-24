import type { MessageEntry } from "../types";

/** Tray popup panel (tray.html entry). Language follows the tray snapshot's
 * app settings through the same provider as the main window. */
export const messages = {
  "tray.panel.aria": ["Agent Switchboard 托盘", "Agent Switchboard tray"],
  "tray.loading": ["正在读取供应商…", "Reading providers…"],
  "tray.emptyGroup": ["暂无供应商", "No providers"],
  "tray.openMain": ["打开主界面", "Open main window"],
  "tray.quit": ["退出", "Quit"],
  "tray.state.queryFailed": ["查询失败", "Query failed"],
  "tray.state.signInRequired": ["待登录", "Sign-in required"],
  "tray.state.reauthRequired": ["需重新登录", "Re-authentication required"],
  "tray.state.stale": ["上次读数", "Last reading"],
  "tray.actionFailed": ["托盘操作失败，请打开主界面检查。", "The tray action failed; open the main window to check."],
  "tray.provider.active": ["{name}，当前供应商", "{name}, current provider"],
  "tray.provider.switchTo": ["切换到 {name}", "Switch to {name}"],
  "tray.provider.switch": ["切换", "Switch"],
  "tray.provider.switching": ["切换中", "Switching"],
} as const satisfies Record<string, MessageEntry>;

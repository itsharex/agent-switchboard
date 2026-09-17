import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const master = join(root, "src", "assets", "app-icon.svg");
const nativeIcons = ["32x32.png", "128x128.png", "128x128@2x.png", "icon.png", "icon.ico", "icon.icns"];
const output = mkdtempSync(join(tmpdir(), "agent-switchboard-icon-"));

try {
  const tauriCli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");
  const result = spawnSync(process.execPath, [tauriCli, "icon", master, "--output", output], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.status !== 0) throw new Error("Tauri icon generation failed.");

  const iconDirectory = join(root, "src-tauri", "icons");
  copyFileSync(master, join(iconDirectory, "icon.svg"));
  for (const icon of nativeIcons) {
    const generated = join(output, icon);
    if (!existsSync(generated)) throw new Error("Missing generated icon: " + icon);
    copyFileSync(generated, join(iconDirectory, icon));
  }
} finally {
  rmSync(output, { force: true, recursive: true });
}

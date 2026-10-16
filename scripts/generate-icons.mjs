import { copyFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const source = join(root, "public", "app-icon.svg");
const output = join(root, "src-tauri", "icons");
const cli = join(root, "node_modules", "@tauri-apps", "cli", "tauri.js");

// 所有平台图标均从界面使用的同一 SVG 生成，不维护第二套绘制代码。
execFileSync(process.execPath, [cli, "icon", source, "--output", output], {
  cwd: root,
  stdio: "inherit",
  windowsHide: true,
});

// 保留已有源图/预览文件名，避免旧工具入口继续拿到旧品牌图。
for (const [from, to] of [
  ["icon.png", "icon-source.png"],
  ["128x128@2x.png", "icon-preview-256.png"],
  ["128x128@2x.png", "icon-32-zoom.png"],
]) {
  copyFileSync(join(output, from), join(output, to));
}

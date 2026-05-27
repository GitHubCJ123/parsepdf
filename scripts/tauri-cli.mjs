import { spawnSync } from "node:child_process";

const args = process.argv.slice(2);
const isDebugBuild = args[0] === "build" && args.includes("--debug");
const hasSigningKey = Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY);

if (isDebugBuild && !hasSigningKey && !args.includes("--config")) {
  args.push("--config", "src-tauri/tauri.debug.conf.json");
}

const command = process.platform === "win32" ? "tauri.cmd" : "tauri";
const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" });

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);

import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { run } = require("@tauri-apps/cli");
const args = process.argv.slice(2);

await run(args, undefined);

if (args[0] === "build") {
	await import("./generate-latest-json.mjs");
}

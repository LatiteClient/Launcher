import { readFile, readdir, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const cargoTomlPath = join(repoRoot, "src-tauri", "Cargo.toml");
const nsisBundleDir = join(
	repoRoot,
	"src-tauri",
	"target",
	"release",
	"bundle",
	"nsis",
);
const outputPath = join(nsisBundleDir, "latest.json");
const repository = "LatiteClient/Launcher";

function readLauncherVersion(cargoToml) {
	const packageSection = cargoToml.match(/\[package\]([\s\S]*?)(?:\n\[|$)/);
	const versionMatch = packageSection?.[1].match(/^\s*version\s*=\s*"([^"]+)"/m);

	if (!versionMatch) {
		throw new Error(`Could not find [package].version in ${cargoTomlPath}.`);
	}

	return versionMatch[1];
}

async function findNsisUpdaterBundle() {
	const entries = await readdir(nsisBundleDir);
	const bundles = entries.filter((entry) => entry.endsWith(".nsis.zip"));

	if (bundles.length !== 1) {
		throw new Error(
			`Expected exactly one .nsis.zip updater bundle in ${nsisBundleDir}, found ${bundles.length}.`,
		);
	}

	return join(nsisBundleDir, bundles[0]);
}

async function main() {
	const cargoToml = await readFile(cargoTomlPath, "utf8");
	const version = readLauncherVersion(cargoToml);
	const bundlePath = await findNsisUpdaterBundle();
	const bundleFileName = basename(bundlePath);
	const githubAssetFileName = bundleFileName.replaceAll(" ", ".");
	const signature = (await readFile(`${bundlePath}.sig`, "utf8")).trim();

	const manifest = {
		version,
		pub_date: new Date().toISOString(),
		platforms: {
			"windows-x86_64": {
				signature,
				url: `https://github.com/${repository}/releases/download/${version}/${githubAssetFileName}`,
			},
		},
	};

	await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
	console.log(`Generated ${outputPath}`);
}

main().catch((error) => {
	console.error(error.message);
	process.exitCode = 1;
});

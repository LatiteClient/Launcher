import { access, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const cargoTomlPath = join(repoRoot, "src-tauri", "Cargo.toml");
const tauriConfigPath = join(repoRoot, "src-tauri", "tauri.conf.json");
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

function toStaticProductName(productName) {
	return productName.trim().replace(/\s+/g, ".");
}

async function fileExists(path) {
	try {
		await access(path);
		return true;
	} catch {
		return false;
	}
}

function escapeRegExp(value) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

async function findUpdaterBundle(version, productName) {
	const entries = await readdir(nsisBundleDir);
	const versionedBundles = entries.filter(
		(entry) =>
			entry.endsWith(".nsis.zip") &&
			entry.includes(`_${version}_`),
	);

	if (versionedBundles.length === 1) {
		return join(nsisBundleDir, versionedBundles[0]);
	}

	if (versionedBundles.length > 1) {
		throw new Error(
			`Expected at most one current-version .nsis.zip updater bundle in ${nsisBundleDir}, found ${versionedBundles.length}.`,
		);
	}

	const staticBundlePattern = new RegExp(
		`^${escapeRegExp(toStaticProductName(productName))}_.+-setup\\.nsis\\.zip$`,
	);
	const staticBundles = entries.filter((entry) =>
		staticBundlePattern.test(entry),
	);

	if (staticBundles.length !== 1) {
		throw new Error(
			`Expected exactly one current or static .nsis.zip updater bundle in ${nsisBundleDir}, found ${staticBundles.length}.`,
		);
	}

	return join(nsisBundleDir, staticBundles[0]);
}

function readArtifactArchitecture(bundleFileName, version, productName) {
	const versionedMatch = bundleFileName.match(
		new RegExp(`_${escapeRegExp(version)}_(.+?)-setup\\.nsis\\.zip$`),
	);
	const staticMatch = bundleFileName.match(
		new RegExp(
			`^${escapeRegExp(toStaticProductName(productName))}_(.+?)-setup\\.nsis\\.zip$`,
		),
	);
	const match = versionedMatch ?? staticMatch;

	if (!match) {
		throw new Error(
			`Could not determine installer architecture from ${bundleFileName}.`,
		);
	}

	return match[1];
}

async function moveReplacing(sourcePath, destinationPath) {
	if (sourcePath === destinationPath) {
		return;
	}

	await rm(destinationPath, { force: true });
	await rename(sourcePath, destinationPath);
}

async function prepareStaticReleaseAssets(version, productName) {
	const sourceUpdaterBundlePath = await findUpdaterBundle(version, productName);
	const sourceUpdaterBundleFileName = basename(sourceUpdaterBundlePath);
	const architecture = readArtifactArchitecture(
		sourceUpdaterBundleFileName,
		version,
		productName,
	);
	const staticBaseName = `${toStaticProductName(productName)}_${architecture}-setup`;

	const sourceInstallerPath = sourceUpdaterBundlePath.replace(/\.nsis\.zip$/, ".exe");
	const sourceSignaturePath = `${sourceUpdaterBundlePath}.sig`;

	for (const path of [
		sourceInstallerPath,
		sourceUpdaterBundlePath,
		sourceSignaturePath,
	]) {
		if (!(await fileExists(path))) {
			throw new Error(`Expected release artifact ${path} to exist.`);
		}
	}

	const staticInstallerPath = join(nsisBundleDir, `${staticBaseName}.exe`);
	const staticUpdaterBundlePath = join(
		nsisBundleDir,
		`${staticBaseName}.nsis.zip`,
	);
	const staticSignaturePath = `${staticUpdaterBundlePath}.sig`;

	await moveReplacing(sourceInstallerPath, staticInstallerPath);
	await moveReplacing(sourceUpdaterBundlePath, staticUpdaterBundlePath);
	await moveReplacing(sourceSignaturePath, staticSignaturePath);

	return {
		staticInstallerPath,
		staticUpdaterBundlePath,
		staticSignaturePath,
	};
}

async function main() {
	const cargoToml = await readFile(cargoTomlPath, "utf8");
	const tauriConfig = JSON.parse(await readFile(tauriConfigPath, "utf8"));
	const version = readLauncherVersion(cargoToml);
	const productName = tauriConfig.package?.productName;

	if (!productName) {
		throw new Error(`Could not find package.productName in ${tauriConfigPath}.`);
	}

	const { staticInstallerPath, staticUpdaterBundlePath, staticSignaturePath } =
		await prepareStaticReleaseAssets(version, productName);
	const updaterBundleFileName = basename(staticUpdaterBundlePath);
	const signature = (await readFile(staticSignaturePath, "utf8")).trim();

	const manifest = {
		version,
		pub_date: new Date().toISOString(),
		platforms: {
			"windows-x86_64": {
				signature,
				url: `https://github.com/${repository}/releases/download/${version}/${updaterBundleFileName}`,
			},
		},
	};

	await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
	console.log(`Prepared ${staticInstallerPath}`);
	console.log(`Prepared ${staticUpdaterBundlePath}`);
	console.log(`Prepared ${staticSignaturePath}`);
	console.log(`Generated ${outputPath}`);
}

main().catch((error) => {
	console.error(error.message);
	process.exitCode = 1;
});

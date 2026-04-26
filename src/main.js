import i18next from "i18next";
import { invoke } from "@tauri-apps/api";
import {
	message as showMessage,
	open as openDialog,
} from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { locale as getSystemLocale } from "@tauri-apps/api/os";
import { open as openUrl } from "@tauri-apps/api/shell";
import { readText as readClipboardText } from "@tauri-apps/api/clipboard";

const AUTO_LOCALE = "auto";
const DEFAULT_LOCALE = "en_US";
const CLOSE_ANIMATION_DURATION_MS = 300;
const CUSTOM_DLL_PATH_OPTION_ID = "custom_dll_path";
const LAUNCHER_LANGUAGE_OPTION_ID = "launcher_language";
const STATUS_EVENT = "inject_status";
const DIALOG_EVENT = "ui_dialog";
const LANGUAGE_FALLBACKS = {
	es: "es_ES",
};
const INITIAL_STATUS_MESSAGE = {
	key: "launcher.status.idle.name",
	args: [],
};

const localeModules = import.meta.glob("./locales/*.json", { eager: true });
const localeRegistry = buildLocaleRegistry(localeModules);
const availableLocales = Object.entries(localeRegistry)
	.map(([id, localeFile]) => ({
		id,
		name: localeFile.name,
	}))
	.sort((left, right) => left.name.localeCompare(right.name));

const launchButton = document.getElementById("launchButton");
const useCustomDllInput = document.getElementById("use_custom_dll");
const customDllInputOption = document.getElementById("customDllInputOption");
const customDllInput = document.getElementById("customDllPath");
const pasteLinkBtn = document.getElementById("pasteLink");
const browseCustomDllButton = document.getElementById("browseCustomDll");
const launcherLanguageSelect = document.getElementById("launcherLanguage");
const launcherLanguageCustomSelect = document.getElementById(
	"launcherLanguageCustomSelect",
);
const launcherLanguageTrigger = document.getElementById(
	"launcherLanguageTrigger",
);
const launcherLanguageMenu = document.getElementById("launcherLanguageOptions");
const launcherLanguageValue = document.getElementById("launcherLanguageValue");
const changelogLink = document.getElementById("changelog");
const githubLink = document.getElementById("github");
const discordLink = document.getElementById("discord");
const openFolderButton = document.getElementById("openFolder");
const launcherContainer = document.querySelector(".launcher");
const latiteBuildInputs = document.querySelectorAll(".latite_build_input");

let currentLocalePreference = AUTO_LOCALE;
let currentSystemLocale = DEFAULT_LOCALE;
let activeStatusMessage = INITIAL_STATUS_MESSAGE;
let statusUpdateInProgress = false;
let pendingStatusUpdate = null;
let closeWindowTimer = null;
let isCloseWindowInProgress = false;
let launcherLanguageMenuDisplayTimer = null;

function buildLocaleRegistry(modules) {
	const registry = {};

	for (const [path, module] of Object.entries(modules)) {
		const localeIdMatch = path.match(/([^/]+)\.json$/);
		const localeId = localeIdMatch?.[1];
		const localeFile = module?.default ?? module;

		if (
			!localeId ||
			!localeFile?.translations ||
			typeof localeFile?.name !== "string"
		) {
			continue;
		}

		registry[localeId] = localeFile;
	}

	return registry;
}

function normalizeLocaleId(value) {
	if (!value) {
		return null;
	}

	const normalized = value.trim().replace(/-/g, "_");
	if (!normalized) {
		return null;
	}

	const [language, region, ...rest] = normalized.split("_");
	if (!language) {
		return null;
	}

	if (!region) {
		return language.toLowerCase();
	}

	return [language.toLowerCase(), region.toUpperCase(), ...rest].join("_");
}

function normalizeLocalePreference(value) {
	if (!value || value === AUTO_LOCALE) {
		return AUTO_LOCALE;
	}

	const normalized = normalizeLocaleId(value);
	return normalized && localeRegistry[normalized] ? normalized : AUTO_LOCALE;
}

function resolveLocaleCandidate(localeId) {
	const normalized = normalizeLocaleId(localeId);
	if (!normalized) {
		return DEFAULT_LOCALE;
	}

	if (localeRegistry[normalized]) {
		return normalized;
	}

	const [languageCode] = normalized.split("_");
	const mappedLocale = LANGUAGE_FALLBACKS[languageCode];

	if (mappedLocale && localeRegistry[mappedLocale]) {
		return mappedLocale;
	}

	return DEFAULT_LOCALE;
}

function resolveLocale(preference, systemLocale) {
	if (preference && preference !== AUTO_LOCALE) {
		return resolveLocaleCandidate(preference);
	}

	return resolveLocaleCandidate(systemLocale);
}

function normalizeTranslationArgs(args) {
	if (Array.isArray(args)) {
		return args;
	}

	if (args === undefined || args === null) {
		return [];
	}

	if (typeof args === "object") {
		return Object.values(args);
	}

	return [args];
}

function interpolateTranslation(template, args) {
	const normalizedArgs = normalizeTranslationArgs(args);
	let argIndex = 0;

	return template.replace(/\{\}/g, () => {
		if (argIndex >= normalizedArgs.length) {
			return "{}";
		}

		const value = normalizedArgs[argIndex];
		argIndex += 1;
		return String(value);
	});
}

function t(key, args) {
	const translation = i18next.t(key, { skipInterpolation: true });
	return interpolateTranslation(translation, args);
}

function normalizeUiMessage(message) {
	if (!message || typeof message !== "object") {
		return INITIAL_STATUS_MESSAGE;
	}

	return {
		key:
			typeof message.key === "string"
				? message.key
				: INITIAL_STATUS_MESSAGE.key,
		args:
			Array.isArray(message.args) ||
			message.args === undefined ||
			message.args === null
				? message.args ?? []
				: normalizeTranslationArgs(message.args ?? message.vars),
	};
}

function translateUiMessage(message) {
	const normalizedMessage = normalizeUiMessage(message);
	return t(normalizedMessage.key, normalizedMessage.args);
}

function createStatusElement(message, className = "status-message") {
	const element = document.createElement("div");
	element.className = className;
	element.dataset.statusKey = message.key;
	element.textContent = translateUiMessage(message);
	return element;
}

function getLauncherLanguageOptions() {
	return [
		{
			value: AUTO_LOCALE,
			label: t("launcher.settings.systemDefault.name"),
		},
		...availableLocales.map((locale) => ({
			value: locale.id,
			label: locale.name,
		})),
	];
}

function getLauncherLanguageOptionButtons() {
	return launcherLanguageMenu
		? Array.from(launcherLanguageMenu.querySelectorAll(".customSelectOption"))
		: [];
}

function focusLauncherLanguageOption(index) {
	const optionButtons = getLauncherLanguageOptionButtons();
	if (!optionButtons.length) {
		return;
	}

	const wrappedIndex = (index + optionButtons.length) % optionButtons.length;
	optionButtons[wrappedIndex]?.focus();
}

function setLauncherLanguageMenuOpen(isOpen, { focusSelected = false } = {}) {
	if (
		!launcherLanguageCustomSelect ||
		!launcherLanguageTrigger ||
		!launcherLanguageMenu
	) {
		return;
	}
	// Clear any pending hide timer
	if (launcherLanguageMenuDisplayTimer) {
		clearTimeout(launcherLanguageMenuDisplayTimer);
		launcherLanguageMenuDisplayTimer = null;
	}

	if (isOpen) {
		// Ensure the menu element is in the layout with its hidden state
		// so opacity/transform transitions can animate.
		launcherLanguageMenu.style.display = "flex";

		// Force a reflow so the browser registers the starting values
		// (opacity:0, transform: translateY(-10px) scale(0.98)) before we add the class.
		// eslint-disable-next-line no-unused-expressions
		launcherLanguageMenu.offsetHeight;

		// Add class to container to trigger the transition to the open state
		launcherLanguageCustomSelect.classList.add("is-open");
	} else {
		// Start close animation by removing open class
		launcherLanguageCustomSelect.classList.remove("is-open");

		// After the close animation, hide the menu so it doesn't take up space
		launcherLanguageMenuDisplayTimer = setTimeout(() => {
			launcherLanguageMenu.style.display = "";
			launcherLanguageMenuDisplayTimer = null;
		}, CLOSE_ANIMATION_DURATION_MS + 50);
	}

	launcherLanguageTrigger.setAttribute("aria-expanded", String(isOpen));
	const centerSettingsEl = document.querySelector(".centersettings");
	// toggle the dropdown-open marker for any existing styles that rely on it
	centerSettingsEl?.classList.toggle("dropdown-open", isOpen);

	// Animate other setting sections to opacity:0, then set display:none after the
	// transition finishes. When closing, restore display and remove the fade class
	// so sections animate back in.
	if (centerSettingsEl) {
		const otherSections = Array.from(
			centerSettingsEl.querySelectorAll(".settingsection:not(#languageSection)"),
		);

		if (isOpen) {
			for (const section of otherSections) {
				// If already hidden via inline style, skip animation
				if (getComputedStyle(section).display === "none") {
					continue;
				}

				// Ensure the element participates in layout before animating
				section.style.display = section.style.display || "";
				// Force reflow so the transition runs
				// eslint-disable-next-line no-unused-expressions
				section.offsetWidth;

				section.classList.add("fade-out");

				// After opacity/transform transition ends, hide the element so it doesn't take space
				const hideAfterTransition = (e) => {
					if (e.propertyName === "opacity" || e.propertyName === "transform") {
						section.style.display = "none";
					}
				};

				section.addEventListener("transitionend", hideAfterTransition, { once: true });

				// Fallback: if transitionend doesn't fire, hide after duration+buffer
				setTimeout(() => {
					if (getComputedStyle(section).opacity === "0" && getComputedStyle(section).display !== "none") {
						section.style.display = "none";
					}
				}, 320);
			}
		} else {
			for (const section of otherSections) {
				// Restore layout participation so the fade-in can animate
				section.style.display = "";
				// Force reflow so removing the class transitions from opacity:0 -> 1
				// eslint-disable-next-line no-unused-expressions
				section.offsetWidth;
				section.classList.remove("fade-out");
			}
		}
	}

	if (isOpen && focusSelected) {
		requestAnimationFrame(() => {
			const selectedOption =
				launcherLanguageMenu.querySelector(".customSelectOption.is-selected") ??
				launcherLanguageMenu.querySelector(".customSelectOption");

			selectedOption?.focus();
		});
	}
}

function syncLauncherLanguageUi(selectedValue = currentLocalePreference) {
	const normalizedValue = normalizeLocalePreference(selectedValue);

	if (launcherLanguageSelect) {
		launcherLanguageSelect.value = normalizedValue;
	}

	if (launcherLanguageCustomSelect) {
		launcherLanguageCustomSelect.dataset.value = normalizedValue;
	}

	let selectedLabel = "";

	getLauncherLanguageOptionButtons().forEach((optionButton) => {
		const isSelected = optionButton.dataset.value === normalizedValue;
		optionButton.classList.toggle("is-selected", isSelected);
		optionButton.setAttribute("aria-selected", String(isSelected));

		if (isSelected) {
			selectedLabel =
				optionButton.dataset.label ?? optionButton.textContent ?? "";
		}
	});

	if (!selectedLabel && launcherLanguageSelect) {
		selectedLabel =
			launcherLanguageSelect.selectedOptions[0]?.textContent ?? "";
	}

	if (launcherLanguageValue) {
		launcherLanguageValue.textContent = selectedLabel;
	}
}

function renderLanguageOptions() {
	if (!launcherLanguageSelect) {
		return;
	}

	const selectedValue = currentLocalePreference;
	const launcherLanguageOptions = getLauncherLanguageOptions();
	launcherLanguageSelect.replaceChildren();
	launcherLanguageMenu?.replaceChildren();

	for (const option of launcherLanguageOptions) {
		launcherLanguageSelect.appendChild(new Option(option.label, option.value));

		if (!launcherLanguageMenu) {
			continue;
		}

		const optionButton = document.createElement("button");
		optionButton.type = "button";
		optionButton.className = "customSelectOption";
		optionButton.dataset.value = option.value;
		optionButton.dataset.label = option.label;
		optionButton.setAttribute("role", "option");
		optionButton.setAttribute("tabindex", "-1");
		optionButton.setAttribute("aria-selected", "false");
		optionButton.textContent = option.label;
		launcherLanguageMenu.appendChild(optionButton);
	}

	syncLauncherLanguageUi(selectedValue);
}

function applyTranslations() {
	document.documentElement.lang = i18next.language.replace("_", "-");
	document.title = t("launcher.meta.title.name");

	document.querySelectorAll("[data-i18n]").forEach((element) => {
		const key = element.dataset.i18n;
		if (!key) {
			return;
		}

		element.textContent = t(key);
	});

	document.querySelectorAll("[data-i18n-placeholder]").forEach((element) => {
		const key = element.dataset.i18nPlaceholder;
		if (!key) {
			return;
		}

		element.setAttribute("placeholder", t(key));
	});

	document.querySelectorAll("[data-i18n-title]").forEach((element) => {
		const key = element.dataset.i18nTitle;
		if (!key) {
			return;
		}

		element.setAttribute("title", t(key));
	});

	renderLanguageOptions();
}

async function showLocalizedDialog(dialog) {
	await showMessage(translateUiMessage(dialog), {
		title: t("launcher.meta.title.name"),
		type: dialog.level,
	});
}

async function showLocalizedError(key, args) {
	await showLocalizedDialog({
		level: "error",
		key,
		args,
	});
}

async function detectSystemLocale() {
	try {
		const systemLocale = await getSystemLocale();
		return (
			normalizeLocaleId(systemLocale) ??
			normalizeLocaleId(navigator.language) ??
			DEFAULT_LOCALE
		);
	} catch {
		return normalizeLocaleId(navigator.language) ?? DEFAULT_LOCALE;
	}
}

async function loadLauncherLanguagePreference() {
	try {
		const savedValue = await invoke("get_string_option", {
			id: LAUNCHER_LANGUAGE_OPTION_ID,
		});
		return normalizeLocalePreference(savedValue);
	} catch (error) {
		console.error("Failed to load launcher language setting:", error);
		return AUTO_LOCALE;
	}
}

async function saveLauncherLanguagePreference(value) {
	await invoke("update_string_option", {
		id: LAUNCHER_LANGUAGE_OPTION_ID,
		value,
	});
}

async function setLocale(localeId) {
	const resolvedLocale = localeRegistry[localeId] ? localeId : DEFAULT_LOCALE;
	await i18next.changeLanguage(resolvedLocale);
	applyTranslations();
	updateStatus(activeStatusMessage, { force: true, skipAnimation: true });
}

async function applyResolvedLocale() {
	await setLocale(resolveLocale(currentLocalePreference, currentSystemLocale));
	syncLauncherLanguageUi(currentLocalePreference);
}

async function initializeLocalization() {
	const resources = Object.fromEntries(
		Object.entries(localeRegistry).map(([localeId, localeFile]) => [
			localeId,
			{
				translation: localeFile.translations,
			},
		]),
	);

	await i18next.init({
		resources,
		lng: DEFAULT_LOCALE,
		fallbackLng: DEFAULT_LOCALE,
		keySeparator: false,
		interpolation: {
			escapeValue: true,
		},
		returnNull: false,
		returnEmptyString: false,
	});

	currentLocalePreference = await loadLauncherLanguagePreference();
	currentSystemLocale = await detectSystemLocale();
	await applyResolvedLocale();
}

async function openCustomDllDialog() {
	return openDialog({
		title: t("launcher.dialog.selectDll.name"),
		multiple: false,
		filters: [
			{
				name: t("launcher.dialog.dllFilter.name"),
				extensions: ["dll"],
			},
		],
	});
}

function isCustomDllUrl(value) {
	try {
		const parsedUrl = new URL(value);
		return parsedUrl.protocol === "http:" || parsedUrl.protocol === "https:";
	} catch {
		return false;
	}
}

function isCustomDllPath(value) {
	return (
		/^[a-z]:[\\/].+\.dll$/i.test(value) ||
		/^\\\\.+\.dll$/i.test(value) ||
		/^\.{1,2}[\\/].+\.dll$/i.test(value)
	);
}

function validateCustomDllValue(value) {
	if (!value) {
		return "launcher.error.customDllMissing.name";
	}

	if (isCustomDllUrl(value) || isCustomDllPath(value)) {
		return null;
	}

	return "launcher.error.customDllInvalid.name";
}

async function buildLaunchRequest() {
	if (!useCustomDllInput?.checked) {
		return {};
	}

	const customDllValue = customDllInput?.value.trim() ?? "";
	const validationKey = validateCustomDllValue(customDllValue);

	if (validationKey) {
		await showLocalizedError(validationKey);
		customDllInput?.focus();
		return null;
	}

	return { dllPath: customDllValue };
}

function setCustomDllFieldEnabled(enabled) {
	if (!customDllInputOption || !customDllInput) {
		return;
	}

	customDllInputOption.classList.toggle("is-disabled", !enabled);
	customDllInput.disabled = !enabled;
	if (browseCustomDllButton) {
		browseCustomDllButton.disabled = !enabled;
	}
}

async function loadSavedCustomDllPath() {
	if (!customDllInput) {
		return;
	}

	try {
		const savedValue = await invoke("get_string_option", {
			id: CUSTOM_DLL_PATH_OPTION_ID,
		});
		customDllInput.value = savedValue;
	} catch (error) {
		console.error("Failed to load custom DLL path:", error);
	}
}

async function saveCustomDllPath(value) {
	await invoke("update_string_option", {
		id: CUSTOM_DLL_PATH_OPTION_ID,
		value,
	});
}

async function updateLauncherLanguagePreference(value) {
	const selectedValue = normalizeLocalePreference(value);

	currentLocalePreference = selectedValue;
	syncLauncherLanguageUi(selectedValue);

	try {
		await saveLauncherLanguagePreference(selectedValue);
		await applyResolvedLocale();
	} catch (error) {
		console.error("Failed to update launcher language setting:", error);
	}
}

function updateStatus(message, options = {}) {
	const normalizedMessage = normalizeUiMessage(message);
	const statusContainer = document.getElementById("statusContainer");

	activeStatusMessage = normalizedMessage;

	if (!statusContainer) {
		return;
	}

	if (statusUpdateInProgress && !options.force) {
		pendingStatusUpdate = {
			message: normalizedMessage,
			options,
		};
		return;
	}

	const currentStatus = statusContainer.querySelector(
		".status-message, #launchSubtext",
	);
	const newStatusText = translateUiMessage(normalizedMessage);

	if (options.force || options.skipAnimation) {
		if (currentStatus) {
			currentStatus.dataset.statusKey = normalizedMessage.key;
			currentStatus.textContent = newStatusText;
			return;
		}

		statusContainer.appendChild(createStatusElement(normalizedMessage));
		return;
	}

	if (
		currentStatus &&
		currentStatus.dataset.statusKey === normalizedMessage.key
	) {
		currentStatus.dataset.statusKey = normalizedMessage.key;
		currentStatus.textContent = newStatusText;
		return;
	}

	if (!currentStatus) {
		statusContainer.appendChild(createStatusElement(normalizedMessage));
		return;
	}

	statusUpdateInProgress = true;
	currentStatus.classList.remove("status-slide-enter", "status-slide-exit");

	const newStatus = createStatusElement(
		normalizedMessage,
		"status-message status-slide-enter",
	);

	requestAnimationFrame(() => {
		currentStatus.classList.add("status-slide-exit");
	});

	setTimeout(() => {
		if (currentStatus.parentNode) {
			currentStatus.remove();
		}

		statusContainer.appendChild(newStatus);
		statusUpdateInProgress = false;

		if (pendingStatusUpdate) {
			const queuedUpdate = pendingStatusUpdate;
			pendingStatusUpdate = null;
			updateStatus(queuedUpdate.message, queuedUpdate.options);
		}
	}, 400);
}

function registerPrimaryEventListeners() {
	launchButton?.addEventListener("click", async () => {
		const request = await buildLaunchRequest();

		if (!request) {
			return;
		}

		invoke("inject", { request }).catch((error) => {
			console.error("Inject failed:", error);
		});
	});

	launchButton?.addEventListener("contextmenu", async (event) => {
		event.preventDefault();

		const selected = await openCustomDllDialog();
		if (selected === null) {
			return;
		}

		const dllPath = Array.isArray(selected) ? selected[0] : selected;

		if (dllPath) {
			invoke("inject", { request: { dllPath } }).catch((error) => {
				console.error("Inject failed:", error);
			});
		}
	});

	browseCustomDllButton?.addEventListener("click", async () => {
		const selected = await openCustomDllDialog();

		if (selected === null || !customDllInput) {
			return;
		}

		customDllInput.value = Array.isArray(selected) ? selected[0] : selected;
	});

	pasteLinkBtn?.addEventListener("click", async () => {
		try {
			const clipboardText = await readClipboardText();
			if (clipboardText && customDllInput) {
				customDllInput.value = clipboardText;
			}
		} catch (error) {
			console.error("Failed to read clipboard:", error);
		}
	});

	customDllInput?.addEventListener("input", async () => {
		try {
			await saveCustomDllPath(customDllInput.value.trim());
		} catch (error) {
			console.error("Failed to auto-save custom DLL path:", error);
		}
	});

	useCustomDllInput?.addEventListener("change", () => {
		setCustomDllFieldEnabled(useCustomDllInput.checked);
	});

	launcherLanguageTrigger?.addEventListener("click", () => {
		const isOpen = launcherLanguageCustomSelect?.classList.contains("is-open");
		setLauncherLanguageMenuOpen(!isOpen);
	});

	launcherLanguageTrigger?.addEventListener("keydown", (event) => {
		if (
			event.key !== "ArrowDown" &&
			event.key !== "ArrowUp" &&
			event.key !== "Enter" &&
			event.key !== " "
		) {
			if (
				event.key === "Escape" &&
				launcherLanguageCustomSelect?.classList.contains("is-open")
			) {
				setLauncherLanguageMenuOpen(false);
			}

			return;
		}

		event.preventDefault();
		setLauncherLanguageMenuOpen(true, { focusSelected: true });
	});

	launcherLanguageMenu?.addEventListener("click", (event) => {
		if (!(event.target instanceof Element)) {
			return;
		}

		const optionButton = event.target.closest(".customSelectOption");
		if (!(optionButton instanceof HTMLButtonElement)) {
			return;
		}

		launcherLanguageTrigger?.focus();
		setLauncherLanguageMenuOpen(false);
		void updateLauncherLanguagePreference(optionButton.dataset.value);
	});

	launcherLanguageMenu?.addEventListener("keydown", (event) => {
		const optionButtons = getLauncherLanguageOptionButtons();
		const focusedIndex = optionButtons.indexOf(document.activeElement);

		switch (event.key) {
			case "ArrowDown":
				event.preventDefault();
				focusLauncherLanguageOption(focusedIndex + 1);
				break;
			case "ArrowUp":
				event.preventDefault();
				focusLauncherLanguageOption(focusedIndex - 1);
				break;
			case "Home":
				event.preventDefault();
				focusLauncherLanguageOption(0);
				break;
			case "End":
				event.preventDefault();
				focusLauncherLanguageOption(optionButtons.length - 1);
				break;
			case "Enter":
			case " ":
				event.preventDefault();

				if (!(document.activeElement instanceof HTMLButtonElement)) {
					return;
				}

				launcherLanguageTrigger?.focus();
				setLauncherLanguageMenuOpen(false);
				void updateLauncherLanguagePreference(
					document.activeElement.dataset.value,
				);
				break;
			case "Escape":
				event.preventDefault();
				launcherLanguageTrigger?.focus();
				setLauncherLanguageMenuOpen(false);
				break;
			case "Tab":
				setLauncherLanguageMenuOpen(false);
				break;
			default:
				break;
		}
	});

	launcherLanguageSelect?.addEventListener("change", async () => {
		await updateLauncherLanguagePreference(launcherLanguageSelect.value);
	});

	document.addEventListener("pointerdown", (event) => {
		if (!launcherLanguageCustomSelect?.classList.contains("is-open")) {
			return;
		}

		if (
			event.target instanceof Node &&
			launcherLanguageCustomSelect.contains(event.target)
		) {
			return;
		}

		setLauncherLanguageMenuOpen(false);
	});

	document.addEventListener("keydown", (event) => {
		if (
			event.key === "Escape" &&
			launcherLanguageCustomSelect?.classList.contains("is-open")
		) {
			launcherLanguageTrigger?.focus();
			setLauncherLanguageMenuOpen(false);
		}
	});

	changelogLink?.addEventListener("click", () => {
		openUrl("https://github.com/LatiteClient/Latite/releases/latest").catch(
			(error) => {
				console.error("Failed to open latest changelog link:", error);
			},
		);
	});

	githubLink?.addEventListener("click", () => {
		openUrl("https://github.com/LatiteClient/Latite").catch((error) => {
			console.error("Failed to open GitHub link:", error);
		});
	});

	discordLink?.addEventListener("click", () => {
		openUrl("https://latite.net/discord").catch((error) => {
			console.error("Failed to open Discord link:", error);
		});
	});

	openFolderButton?.addEventListener("click", () => {
		invoke("open_folder").catch((error) => {
			console.error("Failed to open folder:", error);
		});
	});

	latiteBuildInputs.forEach((input) => {
		input.addEventListener("change", () => {
			if (!input.checked) {
				return;
			}

			invoke("update_latite_build", { build: input.value }).catch((error) => {
				console.error("Failed to update Latite build setting:", error);
			});
		});
	});

	Array.from(document.getElementsByClassName("options_input")).forEach(
		(element) => {
			element.addEventListener("change", () => {
				const id = element.id;

				if (element.type === "checkbox") {
					invoke("update_option", { id, value: element.checked }).catch(
						(error) => {
							console.error("Failed to update option " + id + ":", error);
						},
					);
					return;
				}

				console.error("Non-implemented element type: " + element.type);
			});
		},
	);
}

async function initializeOptionInputs() {
	const optionInputs = Array.from(
		document.getElementsByClassName("options_input"),
	);

	await Promise.all(
		optionInputs.map(async (element) => {
			try {
				const response = await invoke("get_option", { id: element.id });

				if (element.type === "checkbox") {
					element.checked = response;
					return;
				}

				console.error("Non-implemented element type: " + element.type);
			} catch (error) {
				console.error("Failed to load option " + element.id + ":", error);
			}
		}),
	);

	if (useCustomDllInput) {
		setCustomDllFieldEnabled(useCustomDllInput.checked);
	}
}

async function initializeLatiteBuildSetting() {
	try {
		const build = await invoke("get_latite_build");
		const selectedInput = document.querySelector(
			`.latite_build_input[value="${build}"]`,
		);

		if (selectedInput) {
			selectedInput.checked = true;
		}
	} catch (error) {
		console.error("Failed to load Latite build setting:", error);
	}
}

async function registerTauriListeners() {
	await Promise.all([
		listen(STATUS_EVENT, (event) => {
			updateStatus(event.payload);
		}),
		listen(DIALOG_EVENT, (event) => {
			showLocalizedDialog(event.payload).catch((error) => {
				console.error("Failed to show launcher dialog:", error);
			});
		}),
	]);
}

function minimizeWindow() {
	invoke("minimize_window").catch((error) => {
		console.error("Failed to minimize window:", error);
	});
}

function resetCloseWindowAnimationState() {
	if (closeWindowTimer) {
		clearTimeout(closeWindowTimer);
		closeWindowTimer = null;
	}

	launcherContainer?.classList.remove("launcherCloseAnimation");
	isCloseWindowInProgress = false;
}

function startCloseWindowAnimation() {
	launcherContainer?.classList.add("launcherCloseAnimation");
}

function requestCloseWindow() {
	if (isCloseWindowInProgress) {
		return;
	}

	isCloseWindowInProgress = true;
	startCloseWindowAnimation();

	closeWindowTimer = setTimeout(async () => {
		try {
			await invoke("close_window");
		} catch (error) {
			console.error("Failed to close window:", error);
		} finally {
			resetCloseWindowAnimationState();
		}
	}, CLOSE_ANIMATION_DURATION_MS);
}

async function initializeApp() {
	await initializeLocalization();
	await registerTauriListeners();
	registerPrimaryEventListeners();
	await Promise.all([
		initializeOptionInputs(),
		loadSavedCustomDllPath(),
		initializeLatiteBuildSetting(),
		invoke("check_for_updates").catch((error) => {
			console.error("Failed to check for updates:", error);
		}),
	]);
}

initializeApp().catch((error) => {
	console.error("Failed to initialize launcher:", error);
});

window.minimizeWindow = minimizeWindow;
window.requestCloseWindow = requestCloseWindow;

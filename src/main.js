import { invoke } from "@tauri-apps/api";
import { open as openDialog } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/api/shell";

const CUSTOM_DLL_PATH_OPTION_ID = "custom_dll_path";
const launchButton = document.getElementById("launchButton");

launchButton.addEventListener("click", async () => {
  const request = await buildLaunchRequest();

  if (!request) {
    return;
  }

  inject(request);
});

launchButton.addEventListener("contextmenu", async (event) => {
  event.preventDefault();

  const selected = await openDialog({
    title: "Select a DLL to inject",
    multiple: false,
    filters: [
      {
        name: "DLL File",
        extensions: ["dll"],
      },
    ],
  });

  if (selected === null) {
    return;
  }

  const dllPath = Array.isArray(selected) ? selected[0] : selected;

  if (dllPath) {
    inject({ dllPath });
  }
});

function inject(request) {
  invoke("inject", { request }).catch((error) => {
    console.error("Inject failed:", error);
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
    return "Enter a DLL path or URL before launching.";
  }

  if (isCustomDllUrl(value) || isCustomDllPath(value)) {
    return null;
  }

  return "Enter a valid DLL path or URL ending in .dll.";
}

async function buildLaunchRequest() {
  if (!useCustomDllInput?.checked) {
    return {};
  }

  const customDllValue = customDllInput?.value.trim() ?? "";
  const validationError = validateCustomDllValue(customDllValue);

  if (validationError) {
    alert(validationError);
    customDllInput?.focus();
    return null;
  }

  return { dllPath: customDllValue };
}

let statusUpdateInProgress = false;
let pendingStatusUpdate = null;

function updateStatus(payload) {
  const newStatusText = "Status: " + payload;

  if (statusUpdateInProgress) {
    // Queue the update for after current animation completes
    pendingStatusUpdate = payload;
    return;
  }

  const statusContainer = document.getElementById("statusContainer");
  const currentStatus = statusContainer.querySelector(
    ".status-message, #launchSubtext",
  );

  // Extract base message (remove trailing dots for comparison)
  const getBaseMessage = (text) => text.replace(/\.+$/, "");
  const currentBaseMessage = currentStatus
    ? getBaseMessage(currentStatus.textContent)
    : "";
  const newBaseMessage = getBaseMessage(newStatusText);

  if (currentStatus && currentBaseMessage === newBaseMessage) {
    // Same message family (just dots changing), update text without animation
    currentStatus.textContent = newStatusText;
  } else if (!currentStatus) {
    // First status, just add it
    const newStatus = document.createElement("div");
    newStatus.className = "status-message";
    newStatus.textContent = newStatusText;
    statusContainer.appendChild(newStatus);
  } else {
    // Different message, animate transition
    statusUpdateInProgress = true;

    // Remove any existing animation classes to reset state
    currentStatus.classList.remove("status-slide-enter", "status-slide-exit");

    const newStatus = document.createElement("div");
    newStatus.className = "status-message status-slide-enter";
    newStatus.textContent = newStatusText;

    // Trigger exit animation on current status
    requestAnimationFrame(() => {
      currentStatus.classList.add("status-slide-exit");
    });

    // Wait for animation to complete, then swap elements
    setTimeout(() => {
      if (currentStatus.parentNode) {
        currentStatus.remove();
      }
      statusContainer.appendChild(newStatus);

      statusUpdateInProgress = false;

      // Process any queued update
      if (pendingStatusUpdate) {
        const queuedUpdate = pendingStatusUpdate;
        pendingStatusUpdate = null;
        updateStatus(queuedUpdate);
      }
    }, 400);
  }
}

listen("inject_status", (event) => {
  updateStatus(event.payload);
});

let items = document.getElementsByClassName("options_input");

for (let i = 0; i < items.length; i++) {
  let element = items[i];

  invoke("get_option", { id: element.id })
    .then((response) => {
      if (element.type == "checkbox") {
        element.checked = response;
      } else {
        console.error("Non-implemented element type: " + element.type);
      }
    })
    .catch((error) => {
      console.error("Failed to load option " + element.id + ":", error);
    });

  element.addEventListener("change", () => {
    const id = element.id;

    if (element.type == "checkbox") {
      const value = element.checked;
      invoke("update_option", { id, value }).catch((error) => {
        console.error("Failed to update option " + id + ":", error);
      });
    } else {
      console.error("Non-implemented element type: " + element.type);
    }
  });
}

const useCustomDllInput = document.getElementById("use_custom_dll");
const customDllInputOption = document.getElementById("customDllInputOption");
const customDllInput = document.getElementById("customDllPath");
const browseCustomDllButton = document.getElementById("browseCustomDll");
const saveCustomDllButton = document.getElementById("saveCustomDll");

function setCustomDllFieldEnabled(enabled) {
  if (!customDllInputOption || !customDllInput) {
    return;
  }

  customDllInputOption.classList.toggle("is-disabled", !enabled);
  customDllInput.disabled = !enabled;
  browseCustomDllButton.disabled = !enabled;
  saveCustomDllButton.disabled = !enabled;
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

function showCustomDllSavedState() {
  if (!saveCustomDllButton) {
    return;
  }

  const originalLabel = saveCustomDllButton.textContent;
  saveCustomDllButton.textContent = "Saved";
  saveCustomDllButton.disabled = true;

  setTimeout(() => {
    saveCustomDllButton.textContent = originalLabel;
    saveCustomDllButton.disabled = !useCustomDllInput?.checked;
  }, 1200);
}

if (useCustomDllInput) {
  invoke("get_option", { id: "use_custom_dll" })
    .then((enabled) => {
      setCustomDllFieldEnabled(enabled);
    })
    .catch((error) => {
      console.error("Failed to load custom DLL option:", error);
    });

  useCustomDllInput.addEventListener("change", () => {
    setCustomDllFieldEnabled(useCustomDllInput.checked);
  });
}

loadSavedCustomDllPath();

if (browseCustomDllButton) {
  browseCustomDllButton.addEventListener("click", async () => {
    const selected = await openDialog({
      title: "Select a DLL to inject",
      multiple: false,
      filters: [
        {
          name: "DLL File",
          extensions: ["dll"],
        },
      ],
    });

    if (selected === null || !customDllInput) {
      return;
    }

    customDllInput.value = Array.isArray(selected) ? selected[0] : selected;
  });
}

if (saveCustomDllButton) {
  saveCustomDllButton.addEventListener("click", async () => {
    try {
      await saveCustomDllPath(customDllInput?.value.trim() ?? "");
      showCustomDllSavedState();
    } catch (error) {
      console.error("Failed to save custom DLL path:", error);
    }
  });
}

const latiteBuildInputs = document.querySelectorAll(".latite_build_input");

invoke("get_latite_build")
  .then((build) => {
    const selectedInput = document.querySelector(
      `.latite_build_input[value="${build}"]`,
    );

    if (selectedInput) {
      selectedInput.checked = true;
    }
  })
  .catch((error) => {
    console.error("Failed to load Latite build setting:", error);
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

/* External Links - Open in Default Browser */
const changelogLink = document.getElementById("changelog");
const githubLink = document.getElementById("github");
const discordLink = document.getElementById("discord");

if (changelogLink) {
  changelogLink.addEventListener("click", () => {
    openUrl("https://github.com/LatiteClient/Latite/releases/latest").catch(
      (error) => {
        console.error("Failed to open latest changelog link:", error);
      },
    );
  });
}

if (githubLink) {
  githubLink.addEventListener("click", () => {
    openUrl("https://github.com/LatiteClient/Latite").catch((error) => {
      console.error("Failed to open GitHub link:", error);
    });
  });
}

if (discordLink) {
  discordLink.addEventListener("click", () => {
    openUrl("https://latite.net/discord").catch((error) => {
      console.error("Failed to open Discord link:", error);
    });
  });
}

/* Open Folder */
const openFolderBtn = document.getElementById("openFolder");

if (openFolderBtn) {
  openFolderBtn.addEventListener("click", () => {
    invoke("open_folder").catch((error) => {
      console.error("Failed to open folder:", error);
    });
  });
}

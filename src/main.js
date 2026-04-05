import { invoke } from "@tauri-apps/api";
import { open as openDialog } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/api/shell";

const launchButton = document.getElementById("launchButton");
let injectionStatus = "Idle"; // Track current injection status

launchButton.addEventListener("click", async () => {
  // Only allow injection if status is Idle
  if (injectionStatus === "Idle") {
    await handleLaunchClick();
  } else {
    alert("Injection is already in progress. Please wait until Status: Idle");
  }
});

launchButton.addEventListener("contextmenu", async (event) => {
  event.preventDefault();

  // Only allow right-click injection if status is Idle
  if (injectionStatus !== "Idle") {
    alert("Injection is already in progress. Please wait until Status: Idle");
    return;
  }

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
    // Right-click always uses the selected DLL (temporary injection)
    inject({ dllPath });
  }
});

async function inject(request) {
  try {
    console.log("Calling inject with request:", request);
    const result = await invoke("inject", { request });
    console.log("Inject result:", result);
  } catch (error) {
    console.error("Inject failed:", error);
    updateStatus("Idle");
    alert("Injection failed: " + (error.message || error));
  }
}

/* Validate if path or URL is valid */
function isValidPathOrUrl(input) {
  if (!input || input.trim() === "") return false;
  
  const input_trimmed = input.trim();
  
  // Check if it's a URL
  if (input_trimmed.startsWith("http://") || input_trimmed.startsWith("https://")) {
    try {
      new URL(input_trimmed);
      return true;
    } catch (e) {
      return false;
    }
  }
  
  // Check if it's a file path (basic validation)
  if (input_trimmed.includes("\\") || input_trimmed.includes("/")) {
    return input_trimmed.toLowerCase().endsWith(".dll");
  }
  
  return false;
}

/* Handle launch button click with custom DLL validation */
async function handleLaunchClick() {
  const useCustomDll = await invoke("get_option", { id: "use_custom_dll" }).catch(() => false);
  let customDllPath = "";
  
  if (useCustomDll) {
    customDllPath = customDllsInput?.value || "";
    
    if (!customDllPath || customDllPath.trim() === "") {
      // Show alert for empty custom DLL
      updateStatus("No Custom DLL Input");
      alert("You have selected to use Custom DLLs but have not input a path or URL");
      
      // Switch back to Idle after 4 seconds
      setTimeout(() => {
        updateStatus("Idle");
      }, 4000);
      return;
    }
    
    if (!isValidPathOrUrl(customDllPath)) {
      // Show alert for invalid custom DLL
      updateStatus("Invalid Custom DLL");
      alert("You have input an invalid PATH/URL for your custom DLL");
      
      // Switch back to Idle after 4 seconds
      setTimeout(() => {
        updateStatus("Idle");
      }, 4000);
      return;
    }
  }
  
  // Proceed with injection
  let injectionRequest = {};
  
  if (useCustomDll) {
    // Use custom DLL path
    injectionRequest.dllPath = customDllPath;
  }
  
  inject(injectionRequest);
}

let statusUpdateInProgress = false;
let pendingStatusUpdate = null;

function updateStatus(payload) {
  injectionStatus = payload; // Update the injection status
  const newStatusText = "Status: " + payload;
  
  if (statusUpdateInProgress) {
    // Queue the update for after current animation completes
    pendingStatusUpdate = payload;
    return;
  }
  
  const statusContainer = document.getElementById("statusContainer");
  const currentStatus = statusContainer.querySelector(".status-message, #launchSubtext");
  
  // Extract base message (remove trailing dots for comparison)
  const getBaseMessage = (text) => text.replace(/\.+$/, '');
  const currentBaseMessage = currentStatus ? getBaseMessage(currentStatus.textContent) : '';
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
  const statusMessage = event.payload;
  injectionStatus = statusMessage; // Track the injection state
  updateStatus(statusMessage);
  
  // No need to auto-idle - Idle will be emitted from backend when guard is released
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

/* External Links - Open in Default Browser */
const githubLink = document.getElementById("github");
const discordLink = document.getElementById("discord");

if (githubLink) {
  githubLink.addEventListener("click", () => {
    openUrl("https://github.com/LatiteClient/Latite").catch((error) => {
      console.error("Failed to open GitHub link:", error);
    });
  });
}

if (discordLink) {
  discordLink.addEventListener("click", () => {
    openUrl("https://discord.com/invite/latite").catch((error) => {
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

function minimize() {
  invoke("minimize_window").catch((error) => {
    console.error("Failed to minimize window:", error);
  });
}

window.minimize = minimize;

/* Custom DLL Form Handling */
const browseButton = document.getElementById("browseButton");
const customDllsInput = document.getElementById("custom_dlls");
const submitDllButton = document.getElementById("submitDLL");

if (browseButton) {
  browseButton.addEventListener("click", async () => {
    const selected = await openDialog({
      title: "Select a DLL file",
      multiple: false,
      filters: [
        {
          name: "DLL Files",
          extensions: ["dll"],
        },
      ],
    });

    if (selected) {
      const dllPath = Array.isArray(selected) ? selected[0] : selected;
      if (customDllsInput && dllPath) {
        customDllsInput.value = dllPath;
      }
    }
  });
}

if (submitDllButton) {
  submitDllButton.addEventListener("click", async (e) => {
    e.preventDefault();

    // Save custom DLL path
    if (customDllsInput) {
      await invoke("set_string_option", {
        id: "custom_dlls",
        value: customDllsInput.value,
      }).catch((error) => {
        console.error("Failed to save custom DLL path:", error);
      });
    }

    // Success feedback
    const originalText = submitDllButton.value;
    submitDllButton.value = "Saved!";
    setTimeout(() => {
      submitDllButton.value = originalText;
    }, 2000);
  });
}

/* Load preferences on startup */
async function loadPreferences() {
  // Load custom DLL path
  const customDllPath = await invoke("get_string_option", { id: "custom_dlls" }).catch(
    () => ""
  );
  if (customDllsInput && customDllPath) {
    customDllsInput.value = customDllPath;
  }

  // Load use_custom_dll preference and update greyed-out state
  const useCustomDll = await invoke("get_option", { id: "use_custom_dll" }).catch(
    () => false
  );
  updateCustomDllGreyedOut(!useCustomDll);
}

/* Toggle greyed-out class for custom DLL form */
function updateCustomDllGreyedOut(shouldBeGreyedOut) {
  const customDllFormOption = document.querySelector(
    ".settingoption[style*='margin-top: 5px']"
  );
  if (customDllFormOption) {
    if (shouldBeGreyedOut) {
      customDllFormOption.classList.add("greyed-out");
    } else {
      customDllFormOption.classList.remove("greyed-out");
    }
  }
}

/* Add event listener to use_custom_dll checkbox */
const useCustomDllCheckbox = document.getElementById("use_custom_dll");
if (useCustomDllCheckbox) {
  useCustomDllCheckbox.addEventListener("change", () => {
    // Toggle greyed-out class (if checked, remove greyed-out; if unchecked, add greyed-out)
    updateCustomDllGreyedOut(!useCustomDllCheckbox.checked);
  });
}

loadPreferences();
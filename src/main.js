import { invoke } from "@tauri-apps/api";
import { open as openDialog } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/api/shell";

const launchButton = document.getElementById("launchButton");

launchButton.addEventListener("click", () => {
  inject({});
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

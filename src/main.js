import { invoke } from "@tauri-apps/api";
import { open } from "@tauri-apps/api/dialog";

const launchButton = document.getElementById("launchButton");

launchButton.addEventListener("click", () => {
  inject({});
});

launchButton.addEventListener("contextmenu", async (event) => {
  event.preventDefault();

  const selected = await open({
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

import { invoke } from "@tauri-apps/api";

document.getElementById("launchButton").addEventListener("click", () => {
  invoke("inject").catch((error) => {
    console.error("Inject failed:", error);
  });
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

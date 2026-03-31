import { invoke } from "@tauri-apps/api";

invoke("greet", { name: "Tauri" }).then((response) => {
    console.log(response);
});


document.getElementById("launchButton").addEventListener("click", () => {
    invoke("inject").then((response) => {
        console.log(response);
    });
});
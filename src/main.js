import { invoke } from "@tauri-apps/api";

invoke("greet", { name: "Tauri" }).then((response) => {
    console.log(response);
});

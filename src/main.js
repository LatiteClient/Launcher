import { invoke } from "@tauri-apps/api";

invoke("greet", { name: "Tauri" }).then((response) => {
    console.log(response);
});


document.getElementById("launchButton").addEventListener("click", () => {
    invoke("inject").then((response) => {
        console.log(response);
    });
});

let items = document.getElementsByClassName("options_input");

for (let i = 0; i < items.length; i++) {
    let element = items[i];

    invoke("get_option", { id: element.id }).then((response) => {
        console.log(response);
        if (element.type == "checkbox") {
            element.checked = response;
        } else {
            console.error("Non-implemented element type: " + element.type);
        }
    });

    element.addEventListener("change", () => {
        const id = element.id;

        if (element.type == "checkbox") {
            const value = element.checked;
            invoke("update_option", { id, value }).then((response) => {
                console.log(response);
            });
        } else {
            console.error("Non-implemented element type: " + element.type);
        }
    }
    );
}
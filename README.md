# Latite Client Launcher

[![Discord](https://img.shields.io/discord/885656043521179680)](https://discord.gg/GpV3w5tyBs)
[![GitHub release](https://img.shields.io/github/v/release/LatiteClient/Launcher)](https://github.com/LatiteClient/Launcher/releases/latest)

**Latite Client Launcher** is the official launcher for [Latite Client](https://github.com/LatiteClient/Latite), built with [Tauri](https://github.com/tauri-apps/tauri).

<img width="70%" alt="Image of Latite Client Launcher" src="https://github.com/user-attachments/assets/8ee5c2f5-f545-4aa1-baaf-6ae13b5abbfa" />

## Features

- **One-click launch and injection**: Opens Minecraft when needed and injects Latite automatically
- **Multiple Latite builds**: Choose between stable Release, Nightly, and Debug builds
- **Offline-friendly**: Reuses cached Latite builds when a fresh download is unavailable
- **Custom DLL support**: Inject local DLL files or DLL URLs from the launcher
- **Localized interface**: Includes multiple launcher languages with automatic system-language detection
- **Convenience options**: Hide to tray, close after injection, live status updates, and automatic launcher updates
- **Compatibility checks**: Verifies supported Minecraft versions before injecting official Latite builds

## Installation

1. Download the latest installer from the [Launcher releases page](https://github.com/LatiteClient/Launcher/releases/latest).
2. Run the installer.
3. Open **Latite Client Launcher**.
4. Press **Launch** to start Minecraft and inject Latite.

## Building

1. Clone the repository.
2. Install [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install), and the [Tauri prerequisites](https://tauri.app/v1/guides/getting-started/prerequisites).
3. Install dependencies:
   ```console
   npm install
   ```
4. Start the launcher in development mode:
   ```console
   npm run tauri dev
   ```
5. Build a release version:
   ```console
   npm run tauri build
   ```

### Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/)
- [Tauri VS Code extension](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Contributing

We welcome people to contribute code via making a PR (Pull Request) to the Launcher or [Client](https://github.com/LatiteClient/Latite). Just make sure to ping us in our [Discord Server](https://discord.gg/GpV3w5tyBs) if we don't get to reviewing your PR in a timely manner :)

## Community

- [Discord Server](https://discord.gg/GpV3w5tyBs)
- [Twitter](https://twitter.com/LatiteClient)
- [YouTube](https://youtube.com/@LatiteClientMC)

> **Note: These are the only official social medias Latite Client has. If an entity is claiming to be Latite Client and does not have the same socials as the ones listed above, they are impersonating us.**

## FAQ

<details>

<details>
<summary>Why is it flagged as a virus?</summary>
This is a false positive due to DLL injection techniques. Latite is completely safe. <a href="https://latite.net/#faq">Learn more</a>
</details>

<details>
<summary>Can I use this on mobile?</summary>
No — check out our Android project <a href="https://atlasclient.net">Atlas Client</a> instead.
</details>
</details>

[View Full FAQ](https://latiteclient.com/#faq)

## License

By using Latite Client, you agree to our [License Terms](https://raw.githubusercontent.com/LatiteClient/Latite/refs/heads/master/LICENSE).

---------------------------

**Disclaimer**: Latite Client is not affiliated with Mojang or Microsoft in any way, shape, or form. Use at your own risk on multiplayer servers.

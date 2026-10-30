# XenoScope

XenoScope is a Windows desktop research application for investigating reports
of unexplained aerial phenomena. This first milestone establishes the Tauri 2,
React, TypeScript, and Tailwind CSS foundation; research features will be added
incrementally.

## Prerequisites

- Node.js 20.19 or later and npm
- Rust installed with [rustup](https://www.rust-lang.org/tools/install)
- Visual Studio Build Tools 2022 with the **Desktop development with C++**
  workload and a Windows SDK
- Microsoft Edge WebView2 Runtime

See the official [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows)
for current setup details.

## Install dependencies

From the project directory, install the JavaScript dependencies:

```powershell
npm install
```

## Run the desktop app

Start the Vite development server and launch the Tauri desktop window:

```powershell
npm run tauri dev
```

The starter window includes a **Test native bridge** button that calls the
sample Rust command.

## Checks and production build

Build and type-check the frontend:

```powershell
npm run build
```

Build the complete Windows desktop application:

```powershell
npm run tauri build
```

The Tauri development and production commands require the Rust and Windows
build prerequisites listed above.

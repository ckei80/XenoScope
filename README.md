# XenoScope

XenoScope is a Windows desktop research application for investigating reports
of unexplained aerial phenomena. This first milestone establishes the Tauri 2,
React, TypeScript, and Tailwind CSS foundation; research features will be added
incrementally.

## Development Approach & Team

XenoScope is developed through a human–AI collaborative development process. Responsibilities are deliberately divided between product ownership, architectural guidance and implementation.

### Team structure

**Product Owner & Project Lead — Human**

Responsible for the product vision, requirements, priorities, acceptance criteria and final decisions. Owns the direction of the project and approves architectural and implementation changes.

**Solution Architect & AI Mentor — ChatGPT**

Supports the technical direction and architecture of XenoScope, including system design, data models, integration approaches and planned AI research capabilities. Reviews implementation plans, identifies architectural risks and explains technical decisions to support continuous learning.

**AI Developer — GitHub Copilot**

Assists with codebase inspection, implementation planning, coding, test creation, debugging and incremental development under the direction and approval of the Product Owner.

### Development workflow

Development follows an incremental, review-driven process:

1. Define requirements and agree on the intended outcome.
2. Ask GitHub Copilot to inspect the existing codebase and propose an implementation plan.
3. Review the proposed plan and approve the scope before implementation.
4. Implement the approved changes.
5. Validate through tests, code review and application testing.
6. Review the technical decisions and document lessons learned.
7. Preserve completed work through Git commits.

The approach aims to combine human product ownership, AI-assisted architectural guidance and AI-assisted implementation while maintaining clear accountability and technical oversight. The Product Owner retains final approval and decision-making authority.

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

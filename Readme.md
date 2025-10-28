# Eldiron: A Next-Generation Classical RPG Creator

![Eldiron Header](images/eldiron_header.png)

---

![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white) ![macOS](https://img.shields.io/badge/mac%20os-000000?style=for-the-badge&logo=macos&logoColor=F0F0F0) ![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)

[![YouTube](https://img.shields.io/badge/YouTube-FF0000?style=for-the-badge&logo=youtube&logoColor=white)](https://www.youtube.com/channel/UCCmrO356zLQv_m8dPEqBUfA)

[![MIT license](https://img.shields.io/badge/License-MIT-blue.svg)](https://lbesson.mit-license.org/) [![version](https://img.shields.io/badge/version-0.8.80-yellow.svg)](https://shields.io/) [![Discord](https://badgen.net/badge/icon/discord?icon=discord&label)](https://discord.gg/ZrNj6baSZU) [![Twitter](https://badgen.net/badge/icon/twitter?icon=twitter&label)](https://twitter.com/MarkusMoenig)

**Eldiron** is a cross-platform creator for classic retro role-playing games (RPGs). Its primary goal is to enable the creation of RPGs reminiscent of the 1980s and 1990s while incorporating modern features such as multiplayer support, procedural content generation, and more.

Eldiron natively supports **2D** (like Ultima 4/5), **isometric**, and **first-person** RPGs, allowing developers to craft a variety of experiences effortlessly.

Eldiron is open-source and licensed under the **MIT License**.

2D Example           | 3D Example
:-------------------------:|:-------------------------:
![Eldiron Screenshot](images/hideout2d.png)  |  ![Eldiron Screenshot](images/dungeon3d.png)

---

# General Features

- Design 2D maps in a Doom-style editor using vertices, linedefs, and sectors to create textured polygons.
- Quickly *paint* with tiles using the **Rect** tool, which automatically generates the necessary geometry as you work.
- Import your **tilesets**, define **animations** and **metadata**, and expand your tile collection. You can easily set tile properties—such as whether a tile is blocking—at any time. Visit our [Patreon Shop](https://www.patreon.com/c/eldiron/shop) for a growing collection of **tilesets**; supporting us there helps the project.
- Access all commands in Eldiron Creator through the **Action list**. Available actions update automatically based on your geometry selection or UI state, so you always know which commands are ready to use.
- Program character and item behaviors, as well as **advanced shaders**, with intuitive **visual node editors**.
- Eldiron Creator is **cross-platform**, available for **Windows**, **Linux**, and **macOS** in the download section of the [GitHub repo](https://github.com/markusmoenig/Eldiron/releases). Each release includes builds for all clients, including the **Web**. An **Xcode project** is also provided for specialized **iOS**, **macOS**, and **iPadOS** builds.

Eldiron is free and open source under the MIT license — your [support](https://eldiron.com/sponsor) is greatly appreciated.

# 3D Features

- **3D editing** is seamlessly integrated into the editor, allowing you to **create new geometry** or **extrude** and edit **surface profiles** **non-destructively**.  
- The **Action** system is fully integrated with the **geometry workflow**, showing all available actions for your **current selection**.  
- **Edit** and **extrude** surfaces **non-destructively**, including carving holes for **windows**, **recesses**, and **reliefs**.  
- Choose from **isometric**, **first-person**, or **orbit** cameras for flexible editing views.  
- **Gizmos**, **visual helpers**, and a new **GPU-based renderer** are in active development.

3D editing features are currently under heavy development.

# Visual Node System for Behavior and Shaders

- Create behavior visually with a **node-based event system** using simple drag and drop.  
- React to **world events** (like a character entering a sector or a conversation starting) by breaking complex logic into small, manageable tasks.  
- Choose from a wide range of built-in **events** and **commands**.  
- Build **procedural materials** and effects with a similar system for **shaders**.  

Eldiron provides powerful tools to make world building intuitive and fun.

---

## Building Eldiron Creator Locally

If you have [Rust installed](https://www.rust-lang.org/tools/install), you can build Eldiron Creator simply via
`cargo build --release --package creator`

Linux:

Make sure these dependencies are installed: `libasound2-dev` `libatk1.0-dev` `libgtk-3-dev`

## License

The source and all assets I commissioned for Eldiron are licensed under the MIT.

Unless explicitly stated otherwise, any contribution intentionally submitted for inclusion in Eldiron, shall be MIT licensed as above, without any additional terms or conditions.

---

## Sponsor

If you’d like to support the **Eldiron** project, please consider joining my [Patreon](https://www.patreon.com/eldiron), join my [GitHub Sponsor](https://github.com/markusmoenig) or send a [Donation](https://www.paypal.me/markusmoenigos). Your support helps me continue development, commission tilesets, host databases and forums, and more.

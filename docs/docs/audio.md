---
title: "Audio"
sidebar_position: 7
---

This page collects all audio-related workflow and scripting in one place.

## Overview

Audio in Eldiron has two main parts:

- **Assets**: Import and manage audio files in the project tree.
- **Runtime commands**: Play, stop and mix audio buses from server scripts.

See also:

- [Project Tree: Assets](creator/project_tree#assets)
- [Server Commands](characters_items/server_commands)

## Audio Assets

Audio files are managed in the **Assets** section of the project tree.

Supported formats:

- **WAV**
- **OGG**

To import audio:

1. In the project tree, open the **Assets** section.
2. Click **+**.
3. Select **Add Audio Asset**.
4. Choose a `.wav` or `.ogg` file.

The asset name is what you use in scripts, for example:

```eldrin
play_audio("battle_theme")
```

## Runtime Audio Buses (Layers)

When playing audio, use a bus/layer so music and effects can be mixed independently.

Common bus names:

- `music`
- `sfx`
- `ui`
- `ambience`
- `voice`

You can also use custom bus names.

## Server Script Commands

### `play_audio`

Plays an audio asset:

```eldrin
play_audio("door_open")
play_audio("battle_theme", "music", 0.8, true)
```

Parameters:

- `name` (required): audio asset name.
- `bus` (optional): defaults to `"sfx"`.
- `gain` (optional): `0.0..4.0`, defaults to `1.0`.
- `looping` (optional): defaults to `false`.

### `clear_audio`

Stops currently playing audio:

```eldrin
clear_audio("music") // stop only one bus
clear_audio() // stop all buses
```

### `set_audio_bus_volume`

Sets bus volume:

```eldrin
set_audio_bus_volume("music", 0.5)
set_audio_bus_volume("sfx", 1.0)
```

`volume` is clamped to `0.0..4.0`.

## Typical Usage Pattern

```eldrin
// Start background music in loop
play_audio("village_theme", "music", 0.7, true)

// Play one-shot effect
play_audio("sword_hit", "sfx", 1.0, false)

// Duck music for a cutscene
set_audio_bus_volume("music", 0.35)

// Restore normal level
set_audio_bus_volume("music", 0.7)

// Stop music when leaving area
clear_audio("music")
```

## Related References

- [Server Commands](characters_items/server_commands)
- [Project Tree](creator/project_tree)
- [Events](characters_items/events)

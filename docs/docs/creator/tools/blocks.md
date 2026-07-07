---
title: "Block Tool"
sidebar_position: 7
---

The **Block Tool** (keyboard shortcut **`B`**) builds 3D regions from modular block stamps.

Use it for fast dungeon, house, corridor, room, and blockout construction when you want editable 3D Geometry Objects without writing a Builder Graph.

## What It Does

The Block Tool lets you:

- choose block stamps from the **Blocks** dock
- place practical building pieces such as floors, walls, corners, doorways, stairs, ceilings, columns, and solid blocks
- preview each block with a 3D-rendered icon
- stamp one block with a click
- drag line or rectangle strokes of repeated blocks
- replace or erase existing block instances
- rotate blocks in 90-degree steps
- stack blocks on different grid levels
- make component-aware height and width adjustments before stamping

Placed blocks become ordinary editable 3D Geometry Objects. You can continue editing them with the Object, Face, Edge, and Vertex tools.

## Blocks Dock

When the Block Tool is active, the lower dock switches to **Blocks**.

The left side shows the block library as rendered isometric previews. The right side shows details and settings for the selected block:

- **Name**: the selected block stamp
- **Kind**: a short description
- **Footprint**: the grid footprint in cells
- **Pieces**: how many geometry pieces the stamp creates
- **Adjusts**: whether the block reacts to height, width, or both
- **Cell**: world size of one block grid cell
- **Level**: active vertical stack level
- **Rotation**: active 90-degree rotation
- **Height**: remembered height in grid cells for height-aware components
- **Widen**: remembered width expansion in grid cells on each side

The toolbar above the block library contains:

- **Place / Replace / Erase**: choose the edit operation
- **Line / Rect**: choose the drag stroke shape

## Placement Grid

The Block Tool uses its own block grid in 3D views. While the tool is active, this grid replaces the normal edit grid for block placement.

Placement and preview are projected onto the active block-grid plane. This keeps stamps aligned even when the camera is zoomed in or the cursor is over existing geometry.

Use **Level** or the stack shortcuts to move the active block grid up and down.

## Block Stamps

The current starter library includes:

- **Floor Slab**
- **Floor + Wall**
- **Floor + Wall + Ceiling**
- **Floor + Corner**
- **Floor + Doorway**
- **Stairs**
- **Wall**
- **Doorway**
- **Ceiling Slab**
- **Full Block**
- **Large Block**
- **Column**

Composite stamps are intended as the main workflow. For example, **Floor + Wall** places a walkable floor tile and a wall on one cell edge in a single action. Rotation changes which side of the cell receives the wall.

## Place, Replace, And Erase

The operation buttons control what happens when you click or drag:

- **Place**: add the selected block stamp
- **Replace**: remove existing block instances on the affected cells, then place the selected block stamp
- **Erase**: remove existing block instances on the affected cells

Replace and Erase operate on whole block instances. A doorway, corner, or floor+wall stamp may contain multiple Geometry Objects, but it is removed as one block instance.

## Line And Rectangle Strokes

Click places one block stamp.

Drag with **Line** selected to stamp along a straight grid line between the drag start and end cells.

Drag with **Rect** selected to stamp a filled rectangular area.

The 3D overlay previews the whole pending stroke before mouse-up. Erase strokes are shown with red cell outlines.

## Component-Aware Sizing

The Block Tool remembers height and width settings. These settings are applied intelligently per component:

- floors ignore height changes
- walls, posts, columns, ceilings, and lintels react to height changes
- width-aware pieces expand by tile increments
- widened doorways grow the opening without thickening the side posts

This means **make higher** affects wall-like pieces but not floor slabs. **Make wider** affects the useful span of pieces such as walls, floors, ceilings, stairs, and doorway openings.

## Shortcuts

When the 3D view has focus:

- **B**: activate the Block Tool
- **R**: rotate the selected block 90 degrees
- **E**: toggle Place / Erase
- **[**: move the block grid one level down
- **]**: move the block grid one level up
- **h**: make height-aware components one tile higher
- **Shift + H**: make height-aware components one tile lower
- **w**: make width-aware components one tile wider on each side
- **Shift + W**: make width-aware components one tile narrower

## After Stamping

Blocks are baked as editable Geometry Objects. After stamping, use the direct 3D tools to refine them:

- [Object Tool](object): move, resize, duplicate, delete, and assign sources to whole objects
- [Sector / Face Tool](sector): edit faces and assign tile/material sources
- [Linedef / Edge Tool](linedef): edit edges and draw surface details
- [Vertex Tool](vertex): edit vertices

## Tips

- Use **Floor + Wall** and **Floor + Corner** for room outlines.
- Use **Floor + Doorway** where corridors or rooms connect.
- Use **Rect** strokes with Floor Slab for fast room floors.
- Use **Replace** to correct a run of wall cells without erasing manually first.
- Use **Erase** to remove block instances cleanly, especially multi-piece doorways and corners.
- Use **Height** before stamping taller walls; floors in composite stamps stay thin.

## Related Pages

- [Tools Overview](overview)
- [Object Tool](object)
- [Creating 3D Maps: Geometry](/docs/building_maps/creating_3d_maps)

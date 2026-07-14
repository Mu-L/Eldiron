---
title: "3D Paint Tool"
sidebar_position: 12
---

The **3D Paint Tool** (keyboard shortcut **`I`**) paints directly onto 3D region geometry.

Use it when the 3D model provides the playable structure, collision, hit testing, and lighting, but you want a more organic hand-authored surface treatment in isometric, orbit, or first-person views.

## What It Paints

3D Paint stores a persistent paint layer on the region. Strokes are stored by their hit surface, geometry owner, and surface UV position, so they render with the scene instead of being camera-dependent editor marks.

It is intended for:

- moss, grass, dirt, and road breakup
- cracks and ruin detail
- generated brick or tile patterns
- generated arch/trim-like pattern strokes for gates and openings
- surface stamps such as grass, rubble, leaves, flowers, vines, roots, bushes, trees, candles, footprints, and mud
- puddle and wet-look details
- color-only touchups
- material/finish overlays on existing geometry

3D Paint works in every 3D camera: isometric, orbit, and first-person. It is not available in the 2D map view.

## Toolbar

The 3D Paint toolbar contains:

- **Draw / Erase / Pick**: choose the editing operation
- **Visible**: show or hide the authored 3D Paint layer in the editor render
- **Clear All**: remove the current region's 3D Paint layer with undo support
- **No Clip / Surface**: paint freely or constrain the stroke to the starting surface

## Brush Presets

The preset strip shows visual brush thumbnails. Selecting a preset updates its saved settings.

Current preset families include:

- **Material Paint**
- **Brick Pattern**
- **Moss**
- **Crack**
- **Grass Detail**
- **Rubble**
- **Leaves**
- **Flowers**
- **Vines**
- **Roots**
- **Bushes**
- **Tree**
- **Candles**
- **Footprints**
- **Mud**
- **Puddle**
- **Dirt / Color Touchup**

Each preset can keep its own size, opacity, material, finish, brush shape, and palette colors.

## Brush Editor

The left side of the dock previews the selected brush and its recipe layers. The brush shape strip selects the stroke mask, such as solid, soft, dirt, speckle, jagged, scratch, or wash.

The right detail view exposes the editable settings for the selected preset:

- size
- opacity
- one or more Art Palette color slots
- material mode
- pattern settings when the selected preset uses generated patterns
- stamp density, size jitter, and rotation jitter when the selected preset is in stamp mode

Brush colors come from the **Art Palette**. A multi-color brush exposes multiple `Color` slots so terrain, moss, grass, and pattern brushes can use several related colors instead of a single flat swatch.

## Materials and Modes

The material row selects the material family and finish used by the brush. 3D Paint uses the same high-level material library as tiles and palette entries, including families such as stone, dirt, foliage, water, glass, mirror, emissive, fabric, plastic, skin, bone, and wax.

Material mode controls how the stroke interacts with the underlying surface:

- **Coat**: paint over the existing surface while keeping the original surface as the base.
- **Replace**: replace the painted color/material contribution for the covered pixels.
- **Stamp**: place generated scene-aware stamp details instead of a continuous stroke.

Stamp brushes honor the currently selected material family and finish. They also write material pixels for their generated shape, so a candle can be emissive, mud can stay wet, foliage can remain foliage, and custom user material choices are preserved per stamp.

## Pattern Brushes

Pattern brushes use surface hit data to generate aligned patterns, such as tiles or staggered bricks, into the paint layer.

Pattern options include:

- tile, brick, or arch mode
- pattern scale
- mortar width
- generated detail
- color variation

The generated pattern is still paint data, so it can be authored quickly without modeling every brick or tile as separate geometry.

Pattern scale controls the generated tile/brick size independently from the brush size. Brush size controls how much area the stroke covers; pattern scale controls how large the repeated pattern is inside that area.

## Stamp Brushes

Stamp mode places individual generated details at the painted hit point. Stamps store their surface UV, world anchor, owning surface, selected material ID, palette colors, size, opacity, rotation, and variation seed.

This lets stamps:

- stay tied to the scene instead of only the screen
- sort and occlude with the 3D render path
- repaint their material contribution into the material overlay
- erase by nearby stamp, surface clip, and active stamp kind
- remain attached to the same surface in every 3D camera

Drag painting uses the stamp density setting to space repeated stamps. Size jitter and rotation jitter add variation without changing the active brush size.

## Rendering

3D Paint renders in both the editor viewport and the game rendering path when the paint layer is visible.

The render path combines:

- base 3D geometry
- tile and palette-index sources
- high-level material families and finishes
- the authored 3D Paint layer
- generated surface stamps
- stamp material overlays
- the material-aware 3D renderer/post treatment

This keeps the editable 3D structure clean while allowing a more organic, painterly result in every 3D camera.

## Related

- [Palette Tool](/docs/creator/tools/palette)
- [Creating 3D Maps: Geometry](/docs/building_maps/creating_3d_maps)
- [Actions](/docs/creator/actions)

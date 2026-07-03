---
title: "Iso Paint Tool"
sidebar_position: 12
---

The **Iso Paint Tool** (keyboard shortcut **`I`**) lets you paint authored isometric beauty over 3D region geometry.

Use it when the 3D model provides the playable structure, collision, hit testing, and lighting, but you want a more organic hand-authored look in the fixed isometric camera.

## What It Paints

Iso Paint stores a persistent paint layer on the region. Strokes are anchored to the isometric camera and stored with the region/chunk paint data so they render with the scene instead of being temporary editor marks.

It is intended for:

- moss, grass, dirt, and road breakup
- cracks and ruin detail
- generated brick or tile patterns
- puddle and wet-look details
- color-only touchups
- material/finish overlays on existing geometry

Iso Paint is authored for the canonical isometric view. First-person views can still use the base geometry, tiles, and material renderer, but the painted iso layer is designed for the fixed iso presentation.

## Toolbar

The Iso Paint toolbar contains:

- **Draw / Erase / Pick**: choose the editing operation
- **Visible**: show or hide the authored Iso Paint layer in the editor render
- **Clear All**: remove the current region's Iso Paint layer with undo support
- **No Clip / Object**: paint freely or constrain the stroke to the starting object

## Brush Presets

The preset strip shows visual brush thumbnails. Selecting a preset updates its saved settings.

Current preset families include:

- **Material Paint**
- **Brick Pattern**
- **Moss**
- **Crack**
- **Grass Detail**
- **Puddle**
- **Color Only**

Each preset can keep its own size, opacity, material, finish, brush shape, and palette colors.

## Brush Editor

The left side of the dock previews the selected brush and its recipe layers. The brush shape strip selects the stroke mask, such as solid, soft, dirt, speckle, jagged, scratch, or wash.

The right detail view exposes the editable settings for the selected preset:

- size
- opacity
- one or more Art Palette color slots
- material mode
- pattern settings when the selected preset uses generated patterns

Brush colors come from the **Art Palette**. A multi-color brush exposes multiple `Color` slots so terrain, moss, grass, and pattern brushes can use several related colors instead of a single flat swatch.

## Materials and Modes

The material row selects the material family and finish used by the brush. Iso Paint uses the same high-level material library as tiles and palette entries, including families such as stone, dirt, foliage, water, glass, mirror, emissive, fabric, plastic, skin, bone, and wax.

Material mode controls how the stroke interacts with the underlying surface:

- **Coat**: paint over the existing surface while keeping the original surface as the base.
- **Replace**: replace the painted color/material contribution for the covered pixels.

## Pattern Brushes

Pattern brushes use surface hit data to generate aligned patterns, such as tiles or staggered bricks, into the paint layer.

Pattern options include:

- tile or brick mode
- pattern scale
- mortar width
- generated detail
- color variation

The generated pattern is still paint data, so it can be authored quickly without modeling every brick or tile as separate geometry.

## Rendering

Iso Paint renders in both the editor viewport and the game rendering path when the paint layer is visible.

The render path combines:

- base 3D geometry
- tile and palette-index sources
- high-level material families and finishes
- the authored Iso Paint layer
- the material-aware 3D renderer/post treatment

This keeps the editable 3D structure clean while allowing a more organic, painterly isometric result.

## Related

- [Palette Tool](/docs/creator/tools/palette)
- [Creating 3D Maps: Geometry](/docs/building_maps/creating_3d_maps)
- [Actions](/docs/creator/actions)

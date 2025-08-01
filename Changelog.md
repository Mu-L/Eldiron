
# Eldiron Creator v0.8.60

## New Features

### Creator

- The `Data Tool` now supports direct sector selections in the map. Making it easier to select and edit widgets who are mostly data driven.
- Button widgets have new capabilities
  - **active** - Boolean, switch if the widget is active by default
  - **show** - String array of widgets to show when clicked
  - **hide** - String array of widgets to hide when clicked
  - **deactivate** - String array of button widgets to deactivate when clicked
- New `inventory_index` attribute for button widgets to display the inventory item at the given index.
- Intent based actions now also work on items in the inventory (when an intent is active and an inventory button is clicked).
- Material node graphs can now be created for screen widgets, allowing procedural borders and content for screen widgets.

### Server

- New `drop` function to drop a specific item with the given id.
- Refactored some code to make sure all actions / intent are executed correctly on items on the map **and** on items in inventories.
- Major refactoring of the server instancing code. Removes the ref_thread_local dependency and enables rayon parallelism, which in turn finally enables web deployment.

---

# Eldiron Creator v0.8.50

## New Features

### Server

- New 'entered' and 'left' entity system events when entering / leaving named sectors.
- New 'teleport' command which either takes one argument (destination sector name in the same region) or two, the destination sector name and the name of the destination region (which will transfer the character to a different region). The entity will be transferred to the center of the destination sector.
- New `[light]` data attributes to enable light emission for entities and items.
- New 'set_emit_light(True / False)` cmd to enable / disable light emission for an entity or an item.
- New special `active` item attribute which specifies if the item is active or not. On startup (or if the attribute is changed) a new `active` event is send to the item which can than decide what to do based on the value, like enabling / disabling light emission for a torch.
- New `intent` system. Define the current player intent (like "talk", "use", "open" etc.) via the new `intent` parameter for button widgets. Server will send new `intent` events to both entities and items for direction based and click based item interations.
- New `health` config attribute which holds the name of the default entity health attribute, by default `HP`.
- New `mode` entity attribute which holds the current state string of the entity. Set to `active` on entity instantiation and `dead` when the health attribute is <= 0.
- New `death` event send to an entity when the health attribute is <= 0.
- New `id` command which returns the id of the current entity.
- New `took_damage` command (my_id, from_id, damage_amount). This command sends out messages and checks for death.
- New `goto` command (sector name, speed). Makes an NPC go to a sector. Sends `arrived` event on arrival with the name of the sector as value.
- New  `close_in` command (target id, target radius, speed). Makes an NPC close in (in weapon range given by the target radius) of the entity id with the given speed. Once the target is in range a `closed_in` event is send.
- New `kill` event send to the attacker when he kills his target. The value of the event is the id of the dead target.

### Client

- New `intent` command to invoke an intention via key shortcuts (same as actions).

### Creator

- Tileset tool: Preview icons now in the minimap.
- Tilepicker: Icons preview on hover in the minimap.

## Bug Fixes

- Make game widgets honor the global render graph.
- Info viewer did not show item values correctly.
- Changed `Data` tool shortcut from `A` to `D`.
- When adding tiles to the project the background renderer was not updated correctly.
- Adjust Undo / Redo state when switching regions.

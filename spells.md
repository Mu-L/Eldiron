# Spells Design (Draft)

## Goals

- Use one unified spell system for damage, heal, buffs, debuffs, and custom behavior.
- Reuse existing runtime item replication/storage.
- Keep simulation server-authoritative.
- Allow simple data-only spells and advanced scripted spells.

## Runtime Representation

Spells are runtime items with:

- `is_spell = true`

They are treated as special runtime items:

- not pickable
- not inventory-movable
- updated by spell simulation path (movement/collision/lifetime/effects)

## Host Command API

Caster is implicit (current host entity), so no caster argument is needed.

```eldrin
cast_spell(template, target)
cast_spell(template, target, success_pct)
```

- `template`: spell template name (string)
- `target`: entity id or position (`vec2/vec3` as existing VM value style)
- `success_pct`: optional `0..100`, default `100`

Return value:

- spawned spell runtime id, or `-1` if cast fails (chance check / invalid template / invalid target)

## Success / Chance

`success_pct` is a cast success gate before spawn.

- roll done server-side
- if roll fails: no spell spawned, optional `cast_failed` event on caster
- if roll succeeds: spell is spawned and simulated normally

Future extension:

- blend `success_pct` with caster/target stats (focus, dodge, resist) in server logic

## Required Spell Attributes

- `is_spell = true`
- `spell_mode = "projectile" | "instant" | "aoe_persistent"`
- `spell_effect = "damage" | "heal" | "buff" | "debuff" | "custom"`
- `spell_target_filter = "enemy" | "ally" | "self" | "any"`

## Common Optional Attributes

- `spell_amount` (damage/heal magnitude)
- `spell_damage_type` (physical/fire/ice/etc.)
- `spell_speed`
- `spell_max_range`
- `spell_lifetime`
- `spell_radius` (AoE)
- `spell_hit_policy = "first" | "pierce" | "explode"`
- `spell_max_hits`
- `spell_falloff = "none" | "linear"`
- visuals: `tile`, animation attrs, `emit_light`, light attrs
- audio: `cast_sfx`, `travel_sfx`, `impact_sfx`

## Events / Script Hooks

These are regular script events, using the same event model as character and item scripts.
They are sent to the spell template script as normal `event(event_name, value)` events.

Optional spell events:

- `startup`
- `tick`
- `hit`
- `explode`
- `expire`

Default engine behavior applies from attrs.
Hooks can extend/override behavior for complex spells.

## Effect Semantics

- `damage`: apply through existing damage flow (`deal_damage` / `took_damage` path)
- `heal`: increase health, clamp to max health
- `buff` / `debuff`: apply timed status attrs
- `custom`: no default apply; script hook handles effect

## High-Level Server Flow

1. `cast_spell(...)` called from host script.
2. Server validates template + target.
3. Server rolls success check (`success_pct`).
4. On success, spawn runtime item with `is_spell = true`.
5. Spell system updates each tick:
   - movement/homing
   - collision/target checks
   - lifetime/range checks
6. On hit/explode/expire:
   - apply default effect from attrs
   - trigger optional script hook(s)
   - emit VFX/audio messages to client
7. Despawn spell runtime item.

## Networking / Client

Server sends normal item updates for spell runtime objects (already reused).
Client renders spell visuals via item tile/animation/light attrs.
Audio commands are emitted by server events/hooks.

## Minimal First Implementation

1. Add `cast_spell(template, target, success_pct=100)`.
2. Support `spell_mode = projectile` only.
3. Support `spell_effect = damage|heal`.
4. Add `hit` event hook.
5. Block pickup/inventory operations for `is_spell=true`.

This keeps scope small while enabling arrows/fireballs/heal projectiles immediately.

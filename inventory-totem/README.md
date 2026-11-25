# InventoryTotem (Rust / Pumpkin)

A totem of undying anywhere in a player's inventory saves them from lethal damage, as if it were held.
This is a Rust port of [InventoryTotem](https://github.com/Mobilestars/InventoryTotem) 1.0.0 by Mobilestars
(source commit `c0520f4`), built as a Pumpkin Wasm plugin. Licensed Apache-2.0 like the original (see `LICENSE`).

## What players see

The same as the original:

- Lethal damage with a totem in the inventory (storage, armor or offhand) uses one totem instead of killing the player.
- Health is set to 1; fire and fall distance are cleared.
- Effects: Regeneration II (2 s), Absorption II (5 s), Fire Resistance (40 s).
- 30 totem particles and the totem sound play at the player.
- The plugin name, version and description are the same. There are no commands, permissions, config or data files.

The first totem in Bukkit's `getContents()` order is used: storage slots 0-35, then boots, leggings, chestplate,
helmet, offhand.

## Differences inside

- It listens to Pumpkin's `EntityDamageEvent` (blocking, normal priority) instead of Bukkit's.
- "Lethal" compares the event's damage with the player's health. On Paper the original uses
  `getFinalDamage()`, which subtracts armor, resistance and absorption first; on Pumpkin the event carries the
  damage before those. The original running on Pumpkin through PatchBukkit sees the same number, so both versions
  act the same on Pumpkin. Compared with Paper, a player whose armor would have absorbed enough damage to survive
  still uses a totem.
- The player is found by entity id among online players (the event carries only the id).

## Server requirements

- A Pumpkin build whose Wasm host maps sounds by index (Pumpkin-MC/Pumpkin#4160). Without it `play-sound`
  traps and the plugin stops handling events.
- The build is unsigned: allow it for this plugin only in `pumpkin.toml`:

  ```toml
  [plugins.overrides.InventoryTotem]
  allow_unsigned = true
  ```

## Build

```
cargo build --release      # target wasm32-wasip2 is set in .cargo/config.toml
```

Copy `target/wasm32-wasip2/release/inventory_totem.wasm` into the server's `plugins/` folder.

## Tested

The shared test case `InventoryTotem: a totem anywhere in the inventory saves from lethal damage`
(`dev/plugin-tests/tests/20-smp.js` in the network's ops repo) passes against both the original jar (through
PatchBukkit) and this plugin on Pumpkin 0.2.2 (26.3).

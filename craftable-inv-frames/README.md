# craftable-inv-frames

Rust Wasm port of **CraftableInvFrames 2.4.2** (Techdoodle's SurvivalInvisiFrames, forked by SpiritOTHawk) for
Pumpkin. Players craft invisible item frames; this port keeps the commands, permissions, messages and config.

Status: **partial**. The command, config, permissions, placement and crafting-permission gate work as far as the
host API allows. Breaking drops, glow-on-empty and the glow-ink recipe are blocked by missing host API
(see `HOST-API.md`).

## License

`LICENSE.md` is the original's: Techdoodle's code is MIT, SpiritOTHawk's and other contributors' additions are
GPL-3.0-or-later. A port contains both, so the crate is `MIT AND GPL-3.0-or-later` and is distributed under
GPL-3.0-or-later terms.

## What players and admins see (checklist)

| Item | Original | Port |
|---|---|---|
| Plugin name / version | `craftableinvframes` 2.4.2, authors Techdoodle, SpiritOTHawk | same |
| Command | `/iframe` (permission `craftableinvframes.cmd`, default op), usage `/iframe` | done |
| Subcommands | `get` (also no args), `give <player>`, `reload`, `force-recheck`, `setitem`, case-insensitive | done |
| Tab-complete | first argument only, filtered by `.get/.give/.reload/.forcerecheck/.setitem`, sorted | done |
| Permissions | `place` and `craft` default true; `cmd reload forcerecheck get give setitem` default op | done |
| Messages | all 9 keys under `messages.` in config.yml, `%player%` replaced | done |
| Config | `config.yml` with `item-frames-glow`, `recipe`, `itemname-invframe`, `itemname-glow-invframe`, `messages` | read as is; created from the shipped default |
| Item | frame named `§f<itemname-invframe>` with marker `craftableinvframes:invisible` = 1b | done |
| Recipe | 8 frames around the config `recipe` item, gives 8 invisible frames | approximated (item id only) |
| Craft permission | no `craftableinvframes.craft` = no result | via event cancel |
| Place permission | no `craftableinvframes.place` = placement cancelled | done |
| Placed frame | invisible, or glowing+visible when `item-frames-glow` | done |
| Glow frame item (frame + glow ink sac) | dynamic recipe | blocked (HOST-API 3) |
| Drops of a broken frame | keep the invisible item | blocked (HOST-API 2) |
| Glow when empty, hide when filled | on interact / damage / force-recheck | blocked (HOST-API 1) |

## Differences inside

- Config is parsed with `serde_yaml`; missing keys fall back to the shipped defaults in memory and are **not**
  written back (the original runs `copyDefaults` + `saveConfig`). The file is created from the shipped default
  (with comments) when absent.
- `setitem` rewrites only the `recipe:` block and writes `type` only (no `meta`), so the potion type is lost.
- Recipe centre is matched by item id, not by `ExactChoice` (any lingering potion works).
- `reload` re-registers the recipe; the host may keep the first one until restart (no unregister).
- `force-recheck` with `item-frames-glow: true` does nothing (cannot read frame contents); with `false` it makes
  all marked frames invisible and not glowing (the original skips empty ones).
- The tab-complete is a server-side suggestion on one greedy argument, so `/iframe foo` and unknown subcommands
  print `/iframe` like Bukkit's usage message instead of a Brigadier error.
- Frames are found by scanning `world.get_entities()` for the placed entity id (no lookup by id).

## Server requirements

Pumpkin at the revision on `dev/Pumpkin` (`local/teleport-stale-ack`), and in `pumpkin.toml`:

```toml
[plugins.overrides.craftableinvframes]
allow_unsigned = true
```

The plugin asks for `fs.read.data` and `fs.write.data` (config in its data folder).

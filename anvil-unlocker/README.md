# AnvilUnlocker (Rust / Pumpkin)

Removes the anvil's "Too Expensive!" limit. Rust port of
[AnvilUnlocker](https://github.com/cloudnode-pro/AnvilUnlocker) 1.1.3 by Jikoo and Cloudnode (tag `v1.1.3`),
built as a Pumpkin Wasm plugin. Public domain like the original (Unlicense, see `LICENSE`).

**Status: partly blocked.** Pumpkin cannot raise the anvil cost limit yet, so the port loads and mirrors the
original's packet logic but does not lift the limit. See `HOST-API.md` for the two host changes needed.

## What players and admins see (checklist)

| Item | Original | Port |
|---|---|---|
| Plugin name / version / authors | `AnvilUnlocker` / `1.1.3` / Jikoo, Cloudnode | same |
| Commands, aliases, permissions | none | none |
| Messages | none | none |
| Config | `config.yml`: `maximumCost: 32767` (clamped to 41..32767, below 41 reads as 41) | same file, key, default and clamp |
| Data files | none | none |
| Anvil limit raised to `maximumCost` for survival/adventure players | yes | **blocked** (HOST-API.md 1) |
| "Too Expensive!" shown only when cost >= maximum and a second item is present | yes | **blocked** (HOST-API.md 2) |
| Instant-build flag cleared when the anvil closes (not creative) | yes | done |
| Creative players untouched | yes | done |

## Differences inside

- Listens to Pumpkin's `PrepareAnvilEvent` and `InventoryCloseEvent` at normal priority (no monitor priority exists).
- Does not depend on ProtocolLib: the abilities packet is sent with `java-player.send-packet(c-player-abilities)`.
  Bedrock players get no packet (the original goes through ProtocolLib the same way; Geyser translates it there).
- The packet's speeds come straight from the host's abilities (already packet units); the original halves
  Bukkit's `getFlySpeed()` and `getWalkSpeed()`.
- Config lives in the plugin data folder (`plugins/data/AnvilUnlocker/config.yml` on Pumpkin), not
  `plugins/AnvilUnlocker/`. It is created with the default on first load. Copy the original's file over to keep a
  custom value. `config.yml` is read with a line scan, not a YAML parser.
- No `reloadConfig` equivalent (the original only calls it from other plugins); restart to re-read.
- `prepare` handling is a stub: it has only the event's cost (host sends `1`) and cannot see the second slot.

## Server requirements

- Host API in `HOST-API.md` (not in Pumpkin yet).
- Unsigned build, and the data folder permissions it requests:

  ```toml
  [plugins.overrides.AnvilUnlocker]
  allow_unsigned = true
  ```

## Build

```
cargo build --release -p anvil-unlocker
```

Copy `target/wasm32-wasip2/release/anvil_unlocker.wasm` into the server's `plugins/` folder.

## Tested

`dev/plugin-tests/tests/50-anvil-unlocker.js` (ops repo): a survival player combines two items whose cost is above
40 at an anvil and must get a result; the player-abilities packet must show instant build while the result is
available and not after closing. Runs unchanged against the jar and this plugin. The cost test is expected to fail
on the wasm until HOST-API.md is implemented. Build, clippy and fmt are clean.

# Host API ClickMobs needs

Each item: what the port needs from the Pumpkin Wasm host, the Bukkit call it replaces, and what the port does
until it exists. Items 1 to 3 are the ones that change what players see.

1. **Entity snapshot: write an entity to NBT/SNBT and spawn one from it.**
   Want (v0.1/v0.2 `world.wit`, `entity` and `world` resources, additive):
   `entity.save-nbt: func() -> result<string, string>` (SNBT like `EntitySnapshot#getAsString`, or `list<u8>` NBT) and
   `world.spawn-entity-from-nbt: func(entity-type: entity-type, pos: position, nbt: string) -> result<entity, string>`.
   Replaces `Entity#createSnapshot().getAsString()` and `EntityFactory#createEntitySnapshot(String).createEntity(Location)`
   (`SnapshotSaver`). Why: the original keeps the whole mob (health, armor, saddle, trades, color, owner, tags, riders)
   inside the head. Until then the port stores only `id`, `CustomName` and `Age`; everything else is lost on pick up.
   Mobs with riders are refused (`write_error` message) instead of losing the riders.
2. **Drop an item stack at a position / give an item with overflow.** v0.1 has no `world.drop-item` (v0.2 has) and no
   `player.give`. Replaces `Player#getWorld().dropItem` and `PlayerInventory#addItem` in `Utils#setHandOrGive`. Until
   then a full inventory refuses the pick up (`write_error`) instead of dropping the head.
3. **Dispenser event with the item, slot and direction.** `block-dispense-event-data` has only `block-pos` and
   `item-name`. Replaces `BlockPreDispenseEvent` (`getItemStack`, `getSlot`, cancel) plus reading `Directional` and
   writing the dispenser's inventory (`DispenserListener`). Want: `item: item-stack`, `slot: u32`, `direction`, and a
   way to set the slot's item. Until then `enable_dispensers` is read and ignored: dispensers are not handled.
4. **Hand in `player-interact-entity-event-data`.** The original ignores off-hand interactions
   (`event.getHand() != EquipmentSlot.HAND`). Want `hand: hand`. Until then every `interact` action is treated as
   main hand.
5. **Leash state.** `entity.is-leashed: func() -> bool` for the `?leashed` tag (`LivingEntity#isLeashed`). Until then
   the tag never matches.
6. **Persistent data compatible with Bukkit's.** `item-stack.set-custom-data(namespace, key, value)` writes
   `custom_data.<namespace>.<key>`; Bukkit's `PersistentDataContainer` writes `custom_data.PublicBukkitValues["clickmobs:entity"]`
   (booleans as bytes). Heads made by the jar are therefore not recognised by the port and the other way round. Want the
   host (or PatchBukkit) to use `PublicBukkitValues` with the `namespace:key` form for custom data on items and entities,
   which also makes `?customdata` read keys written by Bukkit plugins.
7. **Block particle with block state** (`world.spawn-particle` has no particle data). The original shows a
   `BLOCK` particle of the block below on place; the port shows none.
8. **Mob state for predicates**: tamed/owner for parrots and horses (`?tamed` uses `Tameable`), baby for every
   `Ageable` (only the `ageable-data`, `zombie-data` mobs report it). Want `living-entity.is-tamed`, `is-baby`.
9. **HTTP client** for the Modrinth update check (`check_update`). Until then `check_update` is read and ignored.

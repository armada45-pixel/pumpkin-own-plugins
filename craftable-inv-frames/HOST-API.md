# Host API this port needs (Pumpkin plugin WIT)

Each item: what the Bukkit original calls, the WIT we want, why. Everything below is stubbed or approximated
in `src/lib.rs`; behaviour that is lost is listed in README "Differences inside".

1. **Item frame contents**: `ItemFrame#getItem()` / `setItem`.
   Want on `entity`: `get-frame-item: func() -> option<item-stack>` (none if not an item frame).
   Why: glow-when-empty. The original shows an empty invisible frame glowing and hides it once an item is in it
   (`onPlayerInteractEntity`, `onEntityDamageByEntity`, `forceRecheck`). Without it we cannot tell empty from filled.
2. **Replace the stack of a spawned item entity**: `ItemSpawnEvent#getEntity().setItemStack(..)`.
   Want: `item-spawn-event-data` gets `item: item-stack` (read and write). Today it has only `item-name`.
   Why: a broken invisible frame must drop the invisible frame item (custom name + marker), not a plain frame.
   Alternative: a `drop-item(world, pos, item-stack)` function plus a way to cancel the vanilla drop in
   `hanging-break-event-data`.
3. **Crafting matrix and result**: `PrepareItemCraftEvent#getInventory().getMatrix()` / `setResult(..)`.
   Want: `prepare-item-craft-event-data` gets `matrix: list<option<item-stack>>` and `result: option<item-stack>`.
   Why: the glow invisible frame (invisible frame + one glow ink sac) is decided in code on the marker tag of the
   ingredient. A plain shapeless recipe would also turn ordinary frames into glowing invisible ones, so it is not
   registered. Also needed to block crafting by permission without cancelling the whole event semantics.
4. **Ingredient with data**: `RecipeChoice.ExactChoice(itemStack)`.
   Want: `ingredient` variant `exact(item-stack)` (item id + custom data / components must match).
   Why: the recipe centre is a lingering potion of invisibility from config `recipe`; we can only require the item
   id, so any lingering potion crafts invisible frames.
5. **Remove a recipe**: `Server#removeRecipe` / `recipeIterator().remove()`.
   Want: `recipe-manager.unregister: func(id: string) -> bool`. Why: `/iframe reload` and `setitem` rebuild the
   recipe; today the first registration stays until the next restart.
6. **Read item components for serialising**: `ItemStack` as YAML for `/iframe setitem` (potion type, meta).
   Want: `get-potion-type` or a generic component getter that returns the value (`get-components` gives raw bytes).
   Why: `setitem` should write the held item's meta into `recipe.meta`; we write only `type`.
7. **Persistent data keyed like Bukkit**: confirm `set-custom-data(ns, key, byte)` stores
   `PublicBukkitValues` `craftableinvframes:invisible` so items and frames saved by the jar are still recognised.
   Untested here.

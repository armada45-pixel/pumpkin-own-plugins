# Host API AnvilUnlocker needs from Pumpkin

The original raises the anvil's "Too Expensive!" limit (vanilla 40) to `maximumCost` (default 32767). Pumpkin
hard-codes 40 in `crates/pumpkin-inventory/src/anvil/anvil_screen_handler.rs` (`price = 40`, `repair_cost >= 40`)
and gives plugins no way to change it. Until it does, this port cannot lift the limit.

## 1. Configurable maximum repair cost (blocking)

Replaces Bukkit `AnvilInventory#setMaximumRepairCost(int)` (called from `PrepareAnvilEvent`).

```wit
record prepare-anvil-event-data {
    player: player,
    rename-text: string,
    repair-cost: u32,
    /// Additive: the limit above which the result is "Too Expensive!". Host default 40; the host reads the value
    /// back from the handler's return and uses it for this anvil session.
    maximum-repair-cost: u32,
    /// Additive: true when the second input slot is empty (Bukkit `getItem(1)` null or AIR).
    second-input-empty: bool,
}
```

## 2. The event must fire on every result recompute with the real cost (blocking)

Today `Player::on_rename_item` fires `PrepareAnvilEvent` only when the client sends a rename, with `repair_cost`
hard-coded to `1` and before the screen handler has recomputed the result. Bukkit fires `PrepareAnvilEvent`
whenever input slots or the name change, with the computed cost (`AnvilInventory#getRepairCost`). The plugin needs
that to decide, as the original does, whether to turn instant build on (cost below the maximum, or no second input)
or leave it off so the client shows "Too Expensive!".

Also, the host's own check (`repair_cost >= 40 && !has_infinite_materials` blocks the result) must use the value
from item 1.

## Not needed

- The abilities packet is sent with the existing `java-player.send-packet(c-player-abilities)`.
- `inventory-close-event-data.window-type` already identifies the anvil.

# Host API wanted by afk-pool

## 1. Region lookup (blocker for live behaviour)

The original asks the WorldGuard Java plugin which regions apply at the player's block
(`WorldGuard.getInstance().getPlatform().getRegionContainer().get(world).getApplicableRegions(x, y, z)`).
A Wasm plugin cannot call another plugin's Java API and can read only its own data folder, so on a server where
WorldGuard runs through PatchBukkit the port cannot see live regions.

Current stopgap in the port: it parses a copy of WorldGuard's file at
`<afk-pool data folder>/WorldGuard/worlds/<world>/regions.yml` (cuboid, poly2d, parent regions), cached 10 s.
That needs a manual copy and is stale until WorldGuard saves its file. Without the file no player is ever "in" a
region, so no reward, title or countdown happens (config, commands and `/afkpool test` work).

Proposed fixes, best first:

- **A. Plugin-to-plugin call into a PatchBukkit-hosted Java plugin** through the existing
  `ipc.send-ipc-message(recipient, message)`: PatchBukkit answers messages addressed to a Bukkit plugin name.
  The port would send `{"op":"regions-at","world":"Natural_Lobby","x":1,"y":64,"z":2}` to `WorldGuard` and get back
  the region ids. Replaces: `RegionManager#getApplicableRegions`. Needs a small bridge in PatchBukkit that calls
  WorldGuard (or a tiny Java helper plugin that registers the `regions-at` handler). Live data, no copy.
- **B. Read-only access to another plugin's data folder**, e.g. permission `fs.read.plugin.WorldGuard`
  mounting `plugins/data/.../WorldGuard` read-only. Keeps the file approach but removes the manual copy
  (still stale until WorldGuard autosaves).

## 2. Not needed, listed for the record

- Spigot update check and bStats are not ported (no network permission requested), see README.

//! WorldGuard region lookup from a copy of WorldGuard's own `regions.yml` (see HOST-API.md: a Wasm plugin cannot
//! call the WorldGuard Java plugin). Supports cuboid and poly2d regions and parent regions, as
//! `getApplicableRegions` does.

use serde_yaml_ng::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

// shortcut: a stale-by-up-to-10s file read instead of live WorldGuard data, drop it once the host exposes regions.
const TTL: Duration = Duration::from_secs(10);

enum Shape {
    Cuboid([i32; 3], [i32; 3]),
    Poly(i32, i32, Vec<(i32, i32)>),
    Global,
}

struct Region {
    shape: Shape,
    parent: Option<String>,
}

type World = HashMap<String, Region>;

static CACHE: Mutex<Option<HashMap<String, (Instant, World)>>> = Mutex::new(None);

fn num(v: &Value, key: &str) -> Option<i32> {
    v.get(key)?.as_f64().map(|n| n.floor() as i32)
}

fn vec3(v: &Value) -> Option<[i32; 3]> {
    Some([num(v, "x")?, num(v, "y")?, num(v, "z")?])
}

fn parse(text: &str) -> World {
    let mut out = World::new();
    let Ok(doc) = serde_yaml_ng::from_str::<Value>(text) else {
        return out;
    };
    let Some(regions) = doc.get("regions").and_then(Value::as_mapping) else {
        return out;
    };
    for (id, r) in regions {
        let Some(id) = id.as_str() else { continue };
        let shape = match r.get("type").and_then(Value::as_str) {
            Some("cuboid") => match (r.get("min").and_then(vec3), r.get("max").and_then(vec3)) {
                (Some(a), Some(b)) => Shape::Cuboid(
                    [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
                    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
                ),
                _ => continue,
            },
            Some("poly2d") => {
                let points = r
                    .get("points")
                    .and_then(Value::as_sequence)
                    .map(|s| {
                        s.iter()
                            .filter_map(|p| Some((num(p, "x")?, num(p, "z")?)))
                            .collect()
                    })
                    .unwrap_or_default();
                Shape::Poly(
                    num(r, "min-y").unwrap_or(i32::MIN),
                    num(r, "max-y").unwrap_or(i32::MAX),
                    points,
                )
            }
            Some("global") => Shape::Global,
            _ => continue,
        };
        let parent = r
            .get("parent")
            .and_then(Value::as_str)
            .map(str::to_lowercase);
        out.insert(id.to_lowercase(), Region { shape, parent });
    }
    out
}

impl Shape {
    fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        match self {
            // The global region is never part of an applicable set.
            Shape::Global => false,
            Shape::Cuboid(a, b) => {
                (a[0]..=b[0]).contains(&x)
                    && (a[1]..=b[1]).contains(&y)
                    && (a[2]..=b[2]).contains(&z)
            }
            Shape::Poly(min_y, max_y, pts) => {
                if y < *min_y || y > *max_y || pts.len() < 3 {
                    return false;
                }
                // shortcut: plain even-odd ray cast on block coordinates, WorldGuard's edge rules are not copied.
                let mut inside = false;
                let mut j = pts.len() - 1;
                for i in 0..pts.len() {
                    let ((xi, zi), (xj, zj)) = (pts[i], pts[j]);
                    if (zi > z) != (zj > z)
                        && (x as f64)
                            < f64::from(xj - xi) * f64::from(z - zi) / f64::from(zj - zi)
                                + f64::from(xi)
                    {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
        }
    }
}

/// Is a region called `name` (case-insensitive) applicable at this block of `world`?
/// `dir` is the plugin data folder; the file read is `<dir>/WorldGuard/worlds/<world>/regions.yml`.
pub fn in_region(dir: &str, world: &str, pos: (f64, f64, f64), name: &str) -> bool {
    let name = name.to_lowercase();
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if cache.get(world).is_none_or(|(at, _)| at.elapsed() > TTL) {
        let path = format!("{dir}/WorldGuard/worlds/{world}/regions.yml");
        let regions = std::fs::read_to_string(path)
            .map(|t| parse(&t))
            .unwrap_or_default();
        cache.insert(world.to_string(), (Instant::now(), regions));
    }
    let regions = &cache[world].1;
    let (x, y, z) = (
        pos.0.floor() as i32,
        pos.1.floor() as i32,
        pos.2.floor() as i32,
    );
    regions.iter().any(|(id, r)| {
        if !r.shape.contains(x, y, z) {
            return false;
        }
        // The region itself, or a parent chain (bounded, in case the file has a cycle).
        let mut cur = Some(id.as_str());
        for _ in 0..32 {
            let Some(c) = cur else { return false };
            if c == name {
                return true;
            }
            cur = regions.get(c).and_then(|r| r.parent.as_deref());
        }
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "regions:\n  __global__:\n    type: global\n  parent1:\n    type: cuboid\n    min: {x: 0.0, y: 0.0, z: 0.0}\n    max: {x: 9.0, y: 9.0, z: 9.0}\n  Child:\n    type: cuboid\n    parent: parent1\n    min: {x: 20, y: 0, z: 0}\n    max: {x: 29, y: 9, z: 9}\n";

    #[test]
    fn cuboid_and_parent() {
        let w = parse(FILE);
        assert!(w["parent1"].shape.contains(9, 9, 9) && !w["parent1"].shape.contains(10, 0, 0));
        assert_eq!(w["child"].parent.as_deref(), Some("parent1"));
        assert!(!w["__global__"].shape.contains(0, 0, 0));
    }

    #[test]
    fn poly() {
        let s = Shape::Poly(0, 5, vec![(0, 0), (10, 0), (10, 10), (0, 10)]);
        assert!(s.contains(5, 3, 5) && !s.contains(15, 3, 5) && !s.contains(5, 9, 5));
    }
}

//! LDtk level adjacency, kept in document order within each world.
use super::Project;
use crate::{Result, workflow::invalid};
use serde_json::{Value, json};

struct Bounds {
    iid: Value,
    left: i128,
    right: i128,
    top: i128,
    bottom: i128,
    depth: i64,
}

impl Bounds {
    fn from_level(level: &Value) -> Self {
        let number = |key: &str| i128::from(level[key].as_i64().unwrap_or(0));
        let left = number("worldX");
        let top = number("worldY");
        Self {
            iid: level["iid"].clone(),
            left,
            right: left + number("pxWid"),
            top,
            bottom: top + number("pxHei"),
            depth: level["worldDepth"].as_i64().unwrap_or(0),
        }
    }

    fn touching_direction(&self, other: &Self) -> &'static str {
        if other.left == self.right && other.top == self.bottom {
            "se"
        } else if other.right == self.left && other.top == self.bottom {
            "sw"
        } else if other.left == self.right && other.bottom == self.top {
            "ne"
        } else if other.right == self.left && other.bottom == self.top {
            "nw"
        } else if other.left >= self.right {
            "e"
        } else if other.right <= self.left {
            "w"
        } else if other.bottom <= self.top {
            "n"
        } else {
            "s"
        }
    }
}

fn neighbours_for(
    level_index: usize,
    levels: &[Bounds],
    by_left: &[usize],
    max_width: i128,
) -> Value {
    let here = &levels[level_index];
    // A level starting farther left cannot reach this level's left edge.
    let start = by_left.partition_point(|&i| levels[i].left < here.left - max_width);
    let mut matches = Vec::new();
    for &index in &by_left[start..] {
        let other = &levels[index];
        if other.left > here.right {
            break;
        }
        if index == level_index
            || other.right < here.left
            || other.top > here.bottom
            || other.bottom < here.top
        {
            continue;
        }
        let overlaps = other.left < here.right
            && other.right > here.left
            && other.top < here.bottom
            && other.bottom > here.top;
        let direction = if overlaps {
            if other.depth.abs_diff(here.depth) > 1 {
                continue;
            }
            match other.depth.cmp(&here.depth) {
                std::cmp::Ordering::Equal => "o",
                std::cmp::Ordering::Greater => ">",
                std::cmp::Ordering::Less => "<",
            }
        } else if other.depth == here.depth {
            here.touching_direction(other)
        } else {
            continue;
        };
        matches.push((!overlaps, index, direction));
    }
    // LDtk lists overlapping levels first, then touching levels, each in document order.
    matches.sort_unstable();
    let neighbours = matches
        .into_iter()
        .map(|(_, i, dir)| json!({"levelIid": levels[i].iid, "dir":dir}))
        .collect();
    Value::Array(neighbours)
}

impl Project {
    pub(crate) fn refresh_level_neighbours(&mut self) -> Result<()> {
        for world in self.world_indices() {
            let container = world.map_or(&self.json, |i| &self.json["worlds"][i]);
            let keys = self.level_keys_in_world(world);
            if container["worldLayout"]
                .as_str()
                .is_some_and(|s| s.starts_with("Linear"))
            {
                for key in keys {
                    let level = self
                        .level_mut(key)
                        .ok_or_else(|| invalid("level missing"))?;
                    level["__neighbours"] = Value::Array(Vec::new());
                }
                continue;
            }
            let levels = self
                .level_keys_in_world(world)
                .map(|key| {
                    self.level(key)
                        .map(Bounds::from_level)
                        .ok_or_else(|| invalid("level missing"))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut by_left: Vec<_> = (0..levels.len()).collect();
            by_left.sort_unstable_by_key(|&index| levels[index].left);
            let max_width = levels
                .iter()
                .map(|level| level.right - level.left)
                .max()
                .unwrap_or(0);
            for key in keys {
                self.level_mut(key).expect("validated level")["__neighbours"] =
                    neighbours_for(key.level, &levels, &by_left, max_width);
            }
        }
        Ok(())
    }
}

use crate::{LevelKey, Project, Result, workflow::invalid};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

/// Nonzero cells are always present. Only zero-valued cells need extra state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GridPresence {
    default_present: bool,
    exceptions: Arc<Vec<u64>>,
}

impl Default for GridPresence {
    fn default() -> Self {
        // The source CSV reader inserts entries even for zero-valued cells.
        Self {
            default_present: true,
            exceptions: Arc::default(),
        }
    }
}

impl GridPresence {
    pub(crate) fn empty() -> Self {
        Self {
            default_present: false,
            exceptions: Arc::default(),
        }
    }

    pub(crate) fn contains(&self, index: usize) -> bool {
        let bit = 1 << (index % 64);
        self.default_present
            ^ self
                .exceptions
                .get(index / 64)
                .is_some_and(|word| word & bit != 0)
    }

    pub(crate) fn set(&mut self, index: usize, present: bool) {
        if self.contains(index) == present {
            return;
        }
        let word_index = index / 64;
        let bit = 1 << (index % 64);
        let words = Arc::make_mut(&mut self.exceptions);
        if present != self.default_present {
            if words.len() <= word_index {
                words.resize(word_index + 1, 0);
            }
            words[word_index] |= bit;
        } else {
            words[word_index] &= !bit;
            while words.last() == Some(&0) {
                words.pop();
            }
        }
    }

    pub(crate) fn write(&mut self, index: usize, cell: &mut Value, value: Value) {
        if value == 0 && *cell != 0 {
            self.set(index, false);
        }
        *cell = value;
    }

    pub(crate) fn bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.exceptions.len() * std::mem::size_of::<u64>()
    }
}

pub(crate) type GridPresences = BTreeMap<String, GridPresence>;

pub(crate) fn update_presence<T>(
    presences: &mut GridPresences,
    iid: Option<&str>,
    update: impl FnOnce(&mut GridPresence) -> T,
) -> T {
    let mut state = iid
        .and_then(|iid| presences.remove(iid))
        .unwrap_or_default();
    let result = update(&mut state);
    if let Some(iid) = iid
        && (!state.default_present || !state.exceptions.is_empty())
    {
        presences.insert(iid.into(), state);
    }
    result
}

impl Project {
    pub(crate) fn edit_layer_presence<T>(
        &mut self,
        key: LevelKey,
        index: usize,
        edit: impl FnOnce(&mut Value, &mut GridPresence) -> Result<T>,
    ) -> Result<T> {
        let mut presences = std::mem::take(&mut self.grid_presence);
        let result = self
            .layer_mut(key, index)
            .ok_or_else(|| invalid("layer not found"))
            .and_then(|layer| {
                let iid = layer["iid"].as_str().map(str::to_owned);
                update_presence(&mut presences, iid.as_deref(), |state| edit(layer, state))
            });
        self.grid_presence = presences;
        result
    }
}

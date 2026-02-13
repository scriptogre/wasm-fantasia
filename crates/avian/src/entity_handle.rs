//! Standalone entity handle for use without Bevy.
//!
//! Provides a minimal `Entity` type matching the API surface used internally
//! by Avian's data structures (generational index, hash set).

use core::hash::{Hash, Hasher};
use std::collections::HashSet;

/// A lightweight entity identifier with a generational index.
///
/// This is a stand-in for `bevy::ecs::entity::Entity` when the `bevy` feature
/// is disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entity {
    index: u32,
    generation: EntityGeneration,
}

impl Hash for Entity {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl Entity {
    /// A placeholder entity, used as a default/sentinel value.
    pub const PLACEHOLDER: Self = Self {
        index: u32::MAX,
        generation: EntityGeneration(1),
    };

    /// Returns the index of this entity.
    #[inline]
    pub const fn index_u32(&self) -> u32 {
        self.index
    }

    /// Returns the generation of this entity.
    #[inline]
    pub const fn generation(&self) -> EntityGeneration {
        self.generation
    }

    /// Creates an entity from raw index and generation values.
    #[inline]
    pub const fn from_raw_parts(index: u32, generation: u32) -> Self {
        Self {
            index,
            generation: EntityGeneration(generation),
        }
    }
}

/// The generation counter for an [`Entity`], used to detect stale references.
///
/// Must be `#[repr(transparent)]` wrapping `u32` to support the unsafe
/// transmutes in `SparseSecondaryEntityMap`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct EntityGeneration(pub(crate) u32);

/// A hash set of entities.
///
/// Stand-in for `bevy::ecs::entity::hash_set::EntityHashSet`.
#[derive(Clone, Debug, Default, Eq)]
pub struct EntityHashSet(HashSet<Entity>);

impl EntityHashSet {
    /// Creates an empty `EntityHashSet`.
    ///
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    /// Returns `true` if the set contains the given entity.
    #[inline]
    pub fn contains(&self, entity: &Entity) -> bool {
        self.0.contains(entity)
    }

    /// Inserts an entity into the set.
    #[inline]
    pub fn insert(&mut self, entity: Entity) -> bool {
        self.0.insert(entity)
    }

    /// Removes an entity from the set.
    #[inline]
    pub fn remove(&mut self, entity: &Entity) -> bool {
        self.0.remove(entity)
    }

    /// Clears the set.
    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns the number of entities in the set.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the set is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PartialEq for EntityHashSet {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl FromIterator<Entity> for EntityHashSet {
    fn from_iter<T: IntoIterator<Item = Entity>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

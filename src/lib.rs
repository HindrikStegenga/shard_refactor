#![no_std]
extern crate alloc;

#[cfg(test)]
mod tests;

mod archetype_descriptor;
mod archetype_id;
mod archetype_registry;
mod component;
mod component_descriptor;
mod component_group;
mod component_type_id;
mod constants;
mod entity;
mod entity_registry;
mod fnv1a;
mod registry;
mod shard_registry;

pub use archetype_id::ArchetypeId;
pub use component::Component;
pub use component_type_id::ComponentTypeId;
pub use constants::*;
pub use entity::Entity;
pub use registry::Registry;

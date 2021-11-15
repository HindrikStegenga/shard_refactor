use crate::archetype_descriptor::*;
use alloc::vec::*;

#[derive(Debug, Clone)]
pub(crate) struct Archetype {
    descriptor: ArchetypeDescriptor,
    first_shard_index: u16,
    last_shard_index: u16,
}

impl Archetype {
    pub(crate) fn new(descriptor: ArchetypeDescriptor, first_shard_index: u16) -> Self {
        Self {
            descriptor,
            first_shard_index,
            last_shard_index: first_shard_index,
        }
    }

    #[inline(always)]
    pub unsafe fn set_first_shard_index(&mut self, first_shard_index: u16) {
        self.first_shard_index = first_shard_index;
    }

    #[inline(always)]
    pub fn set_last_shard_index(&mut self, last_shard_index: u16) {
        self.last_shard_index = last_shard_index;
    }

    /// Get archetype's first shard index.
    #[inline(always)]
    pub fn first_shard_index(&self) -> u16 {
        self.first_shard_index
    }
    /// Get archetype's last shard index.
    #[inline(always)]
    pub fn last_shard_index(&self) -> u16 {
        self.last_shard_index
    }

    /// Get a reference to the archetype's descriptor.
    #[inline(always)]
    pub(crate) fn descriptor(&self) -> &ArchetypeDescriptor {
        &self.descriptor
    }
}

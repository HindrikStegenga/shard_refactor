use super::Archetype;
use crate::archetype::metadata::EntityMetadata;
use crate::component_group::*;
use crate::{
    Component, DEFAULT_ARCHETYPE_ALLOCATION_SIZE, MAX_COMPONENTS_PER_ENTITY,
    MAX_ENTITIES_PER_ARCHETYPE,
};
use alloc::alloc::{dealloc, realloc, Layout};
use core::mem::{align_of, size_of};
use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};

impl Archetype {
    /// Returns a reference to a specific component.
    /// # Safety:
    /// - Component type [`C`] must be present in the archetype
    /// - panics otherwise.
    pub(crate) unsafe fn get_component_unchecked<C: Component>(&self, index: u32) -> &C {
        match self
            .descriptor
            .components()
            .binary_search_by_key(&C::ID, |e| e.component_type_id)
        {
            Ok(idx) => &*(self.pointers[idx] as *mut C).offset(index as isize),
            Err(_) => panic!(),
        }
    }

    /// Returns a mutable reference to a specific component.
    /// # Safety:
    /// - Component type [`C`] must be present in the archetype
    /// - panics otherwise.
    pub(crate) unsafe fn get_component_unchecked_mut<C: Component>(
        &mut self,
        index: u32,
    ) -> &mut C {
        match self
            .descriptor
            .components()
            .binary_search_by_key(&C::ID, |e| e.component_type_id)
        {
            Ok(idx) => &mut *(self.pointers[idx] as *mut C).offset(index as isize),
            Err(_) => panic!(),
        }
    }

    /// Reads a specific component from the archetype at the given index.
    /// # Safety:
    /// - Component type [`C`] must be present in the archetype
    /// - panics otherwise.
    pub(crate) unsafe fn read_component_unchecked<C: Component>(&mut self, index: u32) -> C {
        match self
            .descriptor
            .components()
            .binary_search_by_key(&C::ID, |e| e.component_type_id)
        {
            Ok(idx) => {
                core::ptr::read::<C>((self.pointers[idx] as *const C).offset(index as isize))
            }
            Err(_) => panic!(),
        }
    }

    /// Returns a tuple of mutable component slices to the archetype's data.
    /// # Safety:
    /// - Must be called exactly with the component group contained in the archetype.
    /// - a compatible group type is also accepted.
    /// - [`G`] must have a valid archetype descriptor.
    #[inline(always)]
    pub(crate) unsafe fn get_slices_unchecked_exact_mut<'a, G: ComponentGroup<'a>>(
        &'a mut self,
    ) -> G::SliceMutRefTuple {
        debug_assert_eq!(
            G::DESCRIPTOR.archetype().archetype_id(),
            self.descriptor.archetype_id()
        );

        G::slice_unchecked_mut(&self.pointers, self.entity_count as usize)
    }

    /// Returns a tuple of component slices to the archetype's data.
    /// # Safety:
    /// - Must be called exactly with the component group contained in the archetype.
    /// - a compatible group type is also accepted.
    /// - [`G`] must have a valid archetype descriptor.
    #[inline(always)]
    pub(crate) unsafe fn get_slices_unchecked_exact<'a, G: ComponentGroup<'a>>(
        &'a self,
    ) -> G::SliceRefTuple {
        debug_assert_eq!(
            G::DESCRIPTOR.archetype().archetype_id(),
            self.descriptor.archetype_id()
        );

        G::slice_unchecked(&self.pointers, self.entity_count as usize)
    }

    /// Returns the slices for the components in [`G`], provided that archetype itself contains a superset of G.
    /// This function is slower than the exact version, use that if an exact type match is known.
    /// # Safety:
    /// - Only call this with subsets of the types stored in the archetype.
    /// - [`G`] must have a valid archetype descriptor.
    pub unsafe fn get_fuzzy_slices_unchecked<'s, G: ComponentGroup<'s>>(
        &'s self,
    ) -> G::SliceRefTuple {
        debug_assert!(G::DESCRIPTOR.is_valid());
        let pointers = self.get_fuzzy_pointers_unchecked::<G>();
        G::slice_unchecked(&pointers, self.entity_count as usize)
    }

    /// Returns the mutable slices for the components in [`G`], provided that archetype itself contains a superset of G.
    /// This function is slower than the exact version, use that if an exact type match is known.
    /// # Safety:
    /// - Only call this with subsets of the types stored in the archetype.
    /// - [`G`] must have a valid archetype descriptor.
    pub unsafe fn get_fuzzy_slices_unchecked_mut<'s, G: ComponentGroup<'s>>(
        &'s mut self,
    ) -> G::SliceMutRefTuple {
        debug_assert!(G::DESCRIPTOR.is_valid());
        let pointers = self.get_fuzzy_pointers_unchecked::<G>();
        G::slice_unchecked_mut(&pointers, self.entity_count as usize)
    }
}

impl Archetype {
    /// Returns the amount of entities currently stored in the archetype.
    pub fn size(&self) -> u32 {
        self.entity_count
    }

    /// Returns the current capacity of the archetype.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Returns whether the archetype is full or not.
    pub fn is_full(&self) -> bool {
        self.entity_count == self.capacity
    }

    /// Returns a reference to the internal slice storing entity metadata.
    pub(crate) fn entity_metadata(&self) -> &[EntityMetadata] {
        unsafe { &*slice_from_raw_parts(self.entity_metadata, self.entity_count as usize) }
    }

    /// Returns a mutable reference to the internal slice storing entity metadata.
    pub(crate) fn entity_metadata_mut(&mut self) -> &mut [EntityMetadata] {
        unsafe { &mut *slice_from_raw_parts_mut(self.entity_metadata, self.entity_count as usize) }
    }

    /// Pushes a given entity/component-tuple into the archetype's backing memory.
    /// # Safety:
    /// - Must be called exactly with the component group contained in the archetype.
    /// - a compatible group type is also accepted.
    /// - Does not call drop on the given entity.
    /// - Increases the size of the archetype's memory allocations if required.
    /// - If resizing fails, this function will panic.
    pub(crate) unsafe fn push_entity_unchecked<'a, G: ComponentGroup<'a>>(
        &mut self,
        metadata: EntityMetadata,
        entity: G,
    ) -> u32 {
        debug_assert!(G::DESCRIPTOR.is_valid());
        debug_assert_eq!(
            G::DESCRIPTOR.archetype().archetype_id(),
            self.descriptor.archetype_id()
        );

        if self.is_full() {
            let additional_capacity = if self.capacity == 0 {
                DEFAULT_ARCHETYPE_ALLOCATION_SIZE
            } else {
                self.capacity as usize
            };
            self.resize_capacity(additional_capacity as isize);
        }

        let entity_index = self.entity_count;
        self.write_entity_unchecked(entity_index, metadata, entity);
        self.entity_count += 1;
        entity_index
    }

    /// Writes a given entity/component-tuple into the archetype's backing memory.
    /// # Safety:
    /// - Must be called exactly with the component group contained in the archetype.
    /// - a compatible group type is also accepted.
    /// - Does not call drop on the given entity.
    /// - Does not call drop on the entity that already exists at [`index`].
    /// - Assumes the underlying backing memory is sized accordingly to fit the data.
    /// - Does not increase the entity counter.
    /// - Does not check if [`index`] is out of bounds or not.
    pub(crate) unsafe fn write_entity_unchecked<'a, G: ComponentGroup<'a>>(
        &mut self,
        index: u32,
        metadata: EntityMetadata,
        mut entity: G,
    ) {
        debug_assert!(G::DESCRIPTOR.is_valid());
        debug_assert_eq!(
            G::DESCRIPTOR.archetype().archetype_id(),
            self.descriptor.archetype_id()
        );
        let mut pointers = [core::ptr::null_mut(); MAX_COMPONENTS_PER_ENTITY];
        entity.as_sorted_pointers(&mut pointers);
        for i in 0..G::DESCRIPTOR.len() as usize {
            let component = G::DESCRIPTOR.archetype().components().get_unchecked(i);
            let dst_pointer = self
                .pointers
                .get_unchecked(i)
                .offset(component.size as isize * index as isize);

            core::ptr::copy_nonoverlapping::<u8>(
                *pointers.get_unchecked(i),
                dst_pointer,
                component.size as usize,
            );
        }
        *self.entity_metadata_mut().get_unchecked_mut(index as usize) = metadata;
        core::mem::forget(entity);
    }

    /// Swaps the entity at [`index`] and the last entity and drops the now-last entity.
    /// This effectively reduces the size of the archetype by 1, dropping the entity at index.
    /// And moving the previously last entity to the position at index.
    /// If [`index`] is the last element, simply drops it instead without any swaps occurring.
    /// Returns true if a swap occurred, or false if not.
    /// # Safety:
    /// - [`index`] must be smaller than the amount of entities in the archetype.
    pub(crate) unsafe fn swap_drop_unchecked(&mut self, index: u32) -> bool {
        debug_assert!(index < self.entity_count);
        if index == self.entity_count - 1 {
            // Is the last one, so just drop it.
            self.drop_entity(index);
            self.entity_count -= 1;
            false
        } else {
            self.swap_entities(index, self.entity_count - 1);
            self.drop_entity(self.entity_count - 1);
            self.entity_count -= 1;
            true
        }
    }

    /// Swaps the entity at [`index`] and the last entity and returns the now-last entity.
    /// This effectively reduces the size of the archetype by 1, returning the entity at index.
    /// And moving the previously last entity to the position at index.
    /// If [`index`] is the last element, simply returns it instead without any swaps occurring.
    /// Returns true if a swap occurred, or false if not.
    /// # Safety:
    /// - [`index`] must be smaller than the amount of entities in the archetype.
    /// - [`G`] must exactly match the type store in the archetype.
    /// - Ordering of component in [`G`] may be different.
    pub(crate) unsafe fn swap_remove_unchecked<'a, G: ComponentGroup<'a>>(
        &mut self,
        index: u32,
    ) -> (G, bool) {
        debug_assert!(index < self.entity_count);
        if index == self.entity_count - 1 {
            // Is the last one, so just drop it.
            let data: G = self.read_components_exact_unchecked::<G>(index);
            self.entity_count -= 1;
            (data, false)
        } else {
            self.swap_entities(index, self.entity_count - 1);
            let data: G = self.read_components_exact_unchecked(self.entity_count - 1);
            self.entity_count -= 1;
            (data, true)
        }
    }

    /// Swaps the entities at the provided positions.
    /// # Safety:
    /// - [`first`] must be smaller than the amount of entities in the archetype.
    /// - [`second`] must be smaller than the amount of entities in the archetype.
    /// - [`first`] must not be equal to [`last`].
    pub(crate) unsafe fn swap_entities(&mut self, first: u32, second: u32) {
        for (idx, descriptor) in self.descriptor.components().iter().enumerate() {
            let ptr_first = self.pointers[idx].offset(first as isize * descriptor.size as isize);
            let ptr_second = self.pointers[idx].offset(second as isize * descriptor.size as isize);
            core::ptr::swap_nonoverlapping(ptr_first, ptr_second, descriptor.size as usize);
        }
        self.entity_metadata_mut()
            .swap(first as usize, second as usize);
    }

    /// Calls drop on the entity at [`index`].
    /// # Safety:
    /// - [`index`] must be smaller than the amount of entities in the archetype.
    pub(crate) unsafe fn drop_entity(&mut self, index: u32) {
        for (idx, descriptor) in self.descriptor.components().iter().enumerate() {
            (descriptor.fns.drop_handler)(
                self.pointers[idx].offset(index as isize * descriptor.size as isize),
                1,
            );
        }
    }

    /// Drops all the entities in the archetype.
    /// Does not deallocate the memory.
    pub(crate) unsafe fn drop_entities(&mut self) {
        for (idx, descriptor) in self.descriptor.components().iter().enumerate() {
            (descriptor.fns.drop_handler)(self.pointers[idx], self.entity_count as usize);
        }
    }

    /// Reads the component data at [`index`] and returns it.
    /// # Safety:
    /// - [`G`] must be exactly the type stored in the archetype.
    /// - a compatible one also works. (i.e. same archetype, different ordering)
    pub(crate) unsafe fn read_components_exact_unchecked<'a, G: ComponentGroup<'a>>(
        &self,
        index: u32,
    ) -> G {
        let pointers = self.offset_sorted_pointers_unchecked(index);
        G::read_from_sorted_pointers(&pointers)
    }
}

impl Archetype {
    /// Resizes the backing memory by some amount. If this becomes less than or equal to zero,
    /// deallocates all memory.
    /// # Safety:
    /// - Does not call drop on the entities in the backing storage.
    /// - Panics if resizing fails for whatever reason. This leaves the archetype in an undefined state.
    /// - Deallocates if the new capacity is smaller or equal to zero.
    /// - Deallocates if the new capacity exceeds [`MAX_ENTITIES_PER_ARCHETYPE`]. TODO: This is weird?
    pub(super) unsafe fn resize_capacity(&mut self, change_in_entity_count: isize) {
        let old_capacity = self.capacity;
        let new_capacity = old_capacity as isize + change_in_entity_count;
        if new_capacity <= 0 || new_capacity >= MAX_ENTITIES_PER_ARCHETYPE as isize {
            self.dealloc();
            return;
        }
        let new_capacity = new_capacity as usize;

        let layout = Layout::from_size_align_unchecked(
            size_of::<EntityMetadata>() * old_capacity as usize,
            align_of::<EntityMetadata>(),
        );
        self.entity_metadata = realloc(
            self.entity_metadata as *mut u8,
            layout,
            size_of::<EntityMetadata>() * new_capacity,
        ) as *mut EntityMetadata;
        assert_ne!(self.entity_metadata, core::ptr::null_mut());
        for (index, pointer) in self.pointers[0..self.descriptor.len() as usize]
            .iter_mut()
            .enumerate()
        {
            let component_type = &self.descriptor.components()[index];
            let layout = alloc::alloc::Layout::from_size_align_unchecked(
                component_type.size as usize * old_capacity as usize,
                component_type.align as usize,
            );
            *pointer = realloc(
                *pointer,
                layout,
                component_type.size as usize * new_capacity,
            );
            assert_ne!(*pointer, core::ptr::null_mut());
        }
        self.capacity = new_capacity as u32;
    }

    /// Deallocates the backing memory and sets capacity to zero.
    /// # Safety:
    /// - Does not call drop on the entities in the backing storage.
    pub(super) unsafe fn dealloc(&mut self) {
        for (index, pointer) in self.pointers[0..self.descriptor.len() as usize]
            .iter_mut()
            .enumerate()
        {
            if *pointer == core::ptr::null_mut() {
                return;
            }
            let component_type = &self.descriptor.components()[index];
            let layout = alloc::alloc::Layout::from_size_align_unchecked(
                component_type.size as usize * self.capacity as usize,
                component_type.align as usize,
            );
            dealloc(*pointer, layout);
            *pointer = core::ptr::null_mut();
        }
        let layout = Layout::from_size_align_unchecked(
            size_of::<EntityMetadata>() * self.capacity as usize,
            align_of::<EntityMetadata>(),
        );
        dealloc(self.entity_metadata as *mut u8, layout);
        self.entity_metadata = core::ptr::null_mut();
        self.capacity = 0;
    }

    /// Returns the pointers, offset by [`index`] elements.
    #[inline(always)]
    unsafe fn offset_sorted_pointers_unchecked(
        &self,
        index: u32,
    ) -> [*mut u8; MAX_COMPONENTS_PER_ENTITY] {
        let mut pointers = [core::ptr::null_mut(); MAX_COMPONENTS_PER_ENTITY];
        for (c_idx, pointer) in self.pointers[0..self.descriptor.len() as usize]
            .iter()
            .enumerate()
        {
            pointers[c_idx] =
                pointer.offset(self.descriptor.components()[c_idx].size as isize * index as isize);
        }
        pointers
    }

    /// Copies common components between two archetypes.
    pub(crate) unsafe fn copy_common_components_between_archetypes_unchecked(
        source: &Archetype,
        source_index: u32,
        destination: &mut Archetype,
        destination_index: u32,
    ) {
        for (source_c_idx, source_component) in source.descriptor.components().iter().enumerate() {
            for (destination_c_idx, destination_component) in
                destination.descriptor.components().iter().enumerate()
            {
                if source_component.component_type_id != destination_component.component_type_id {
                    continue;
                }
                core::ptr::copy_nonoverlapping(
                    source.pointers[source_c_idx]
                        .offset(source_component.size as isize * source_index as isize),
                    destination.pointers[destination_c_idx]
                        .offset(destination_component.size as isize * destination_index as isize),
                    source_component.size as usize,
                );
            }
        }
    }

    /// Returns the pointers for the components in [`G`], provided that archetype itself contains a superset of G.
    /// This function is slower than the exact version, use that if an exact type match is known.
    /// # Safety:
    /// - Only call this with subsets of the types stored in the shard.
    unsafe fn get_fuzzy_pointers_unchecked<'a, G: ComponentGroup<'a>>(
        &'a self,
    ) -> [*mut u8; MAX_COMPONENTS_PER_ENTITY] {
        let mut pointers = [core::ptr::null_mut(); MAX_COMPONENTS_PER_ENTITY];
        for (index, descriptor) in G::DESCRIPTOR.archetype().components().iter().enumerate() {
            'inner_loop: for check_index in index..self.descriptor.len() as usize {
                if self
                    .descriptor
                    .components()
                    .get_unchecked(check_index)
                    .component_type_id
                    .into_u16()
                    == descriptor.component_type_id.into_u16()
                {
                    *pointers.get_unchecked_mut(index) = *self.pointers.get_unchecked(check_index);
                    break 'inner_loop;
                }
            }
        }
        pointers
    }
}

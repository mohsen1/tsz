```rust
// src/parser/arena.rs

//! A fast, bump-pointer allocator arena designed for zero-copy parsing.
//!
//! This module provides an `Arena` that grows by allocating large chunks of memory
//! (pages) and serving allocations out of them using a bump pointer. This strategy
//! is highly efficient for AST construction and string interning where objects
//! have similar lifetimes.

use std::alloc::{self, Layout};
use std::ptr::NonNull;
use std::marker::PhantomData;
use std::mem;
use std::slice;

/// Configuration for the Arena growth strategy.
const DEFAULT_PAGE_SIZE: usize = 4 * 1024; // 4 KB

/// An error that can occur during allocation.
#[derive(Debug)]
pub enum ArenaError {
    /// Allocation failed due to layout constraints or OOM.
    AllocationFailed,
    /// Requested alignment exceeds supported maximum.
    BadAlignment,
}

/// A specialized bump-allocator arena optimized for zero-copy parsing.
///
/// # Safety
/// The returned references are bound to the lifetime of the Arena itself.
/// The Arena must not be moved or dropped while references are active.
pub struct Arena {
    /// List of allocated chunks. We keep these to deallocate them later.
    /// Chunks are stored in reverse order of allocation (newest first) for convenience,
    /// though logically order doesn't strictly matter for the bump strategy within a chunk.
    chunks: Vec<Chunk>,
    
    /// Pointer to the start of the current allocation region.
    /// If `None`, no memory is currently allocated.
    current_start: Option<NonNull<u8>>,
    
    /// Pointer to the next byte to be allocated.
    current_end: NonNull<u8>, // Points to the byte *after* the last valid byte (capacity limit)
    current_ptr: NonNull<u8>, // Points to the next free byte
}

/// Represents a single contiguous block of memory allocated from the system.
struct Chunk {
    ptr: NonNull<u8>,
    size: usize,
}

impl Drop for Chunk {
    fn drop(&mut self) {
        // SAFETY: `ptr` was allocated via `alloc::alloc` with the layout matching `size`.
        // We must use the size to reconstruct the layout for deallocation.
        let layout = unsafe { Layout::from_size_align_unchecked(self.size, 1) };
        unsafe { alloc::dealloc(self.ptr.as_ptr(), layout) };
    }
}

impl Arena {
    /// Creates a new, empty Arena.
    pub fn new() -> Self {
        Arena {
            chunks: Vec::new(),
            current_start: None,
            // Initialize pointers to a dummy null address; they will be valid
            // once the first allocation happens or `ensure_capacity` is called.
            current_end: NonNull::dangling(),
            current_ptr: NonNull::dangling(),
        }
    }

    /// Allocates a new chunk of memory and makes it the current active chunk.
    ///
    /// # Arguments
    /// * `min_size`: The minimum size required for the new chunk.
    fn grow(&mut self, min_size: usize) {
        // Calculate a good size for the next chunk. We double the size of the previous chunk
        // or start at DEFAULT_PAGE_SIZE, ensuring we cover `min_size`.
        let new_size = (DEFAULT_PAGE_SIZE).max(min_size).next_power_of_two();
        
        let layout = match Layout::from_size_align(new_size, 1) {
            Ok(l) => l,
            Err(_) => return, // Handle size overflow gracefully by doing nothing (will fail on alloc)
        };

        let ptr = unsafe { alloc::alloc(layout) };
        
        if ptr.is_null() {
            // OOM handling: In a standard library context, we might panic or abort.
            // Here we rely on the caller checking null pointers or the allocator panic.
            // Since `alloc` technically returns null on OOM, we check here.
            // Note: Standard Rust allocators usually abort on OOM, but custom ones might return null.
            // For robustness, we assume the system allocator panics, but we check anyway.
            panic!("Arena OOM: Failed to allocate {} bytes", new_size);
        }

        let nn_ptr = NonNull::new(ptr).expect("Allocation pointer should not be null");
        
        self.chunks.push(Chunk { ptr: nn_ptr, size: new_size });
        
        self.current_start = Some(nn_ptr);
        self.current_ptr = nn_ptr;
        // Safety: `ptr.add(new_size)` is within the same allocated object.
        self.current_end = unsafe { NonNull::new_unchecked(ptr.add(new_size)) };
    }

    /// Ensures that the current chunk has at least `size` bytes available.
    /// If not, allocates a new chunk.
    #[inline]
    fn ensure_capacity(&mut self, size: usize) {
        // Check if we have space in the current chunk.
        // `current_ptr` is the start of free space, `current_end` is the end of the chunk.
        let available = self.current_end.as_ptr() as usize - self.current_ptr.as_ptr() as usize;
        
        if available < size {
            self.grow(size);
        }
    }

    /// Allocates space for an item of type `T` and returns a mutable reference to it.
    ///
    /// # Safety
    /// The returned reference is valid for the lifetime of the Arena.
    /// The memory is zero-initialized.
    #[inline]
    pub fn alloc<T>(&mut self) -> &'static mut T 
    where
        T: Default, // Using Default to ensure initialization logic is simple, or handle raw
    {
        let size = mem::size_of::<T>();
        let align = mem::align_of::<T>();

        // Calculate padding needed to satisfy alignment
        let offset = self.offset_for_alignment(align);
        let total_size = offset + size;

        self.ensure_capacity(total_size);

        // Advance pointer by padding
        let aligned_ptr = unsafe { self.current_ptr.as_ptr().add(offset) };
        
        // Update bump pointer
        // Safety: `ensure_capacity` guarantees `total_size` fits.
        self.current_ptr = unsafe { NonNull::new_unchecked(aligned_ptr.add(size)) };

        // Write default value
        let ptr = aligned_ptr as *mut T;
        unsafe {
            ptr.write(T::default());
            &mut *ptr
        }
    }

    /// Allocates a raw byte slice of a specific length.
    /// The memory is uninitialized.
    ///
    /// This is useful for copying strings into the arena directly.
    #[inline]
    pub fn alloc_bytes(&mut self, len: usize) -> &'static mut [u8] {
        self.ensure_capacity(len);
        
        let ptr = self.current_ptr.as_ptr();
        self.current_ptr = unsafe { NonNull::new_unchecked(ptr.add(len)) };
        
        unsafe { slice::from_raw_parts_mut(ptr, len) }
    }

    /// Allocates space for a `T` with a specific byte length.
    /// This is often used for DST (Dynamically Sized Types) or fat pointers like `str`.
    /// For `str`, `T` would be `str` (size 0) but we actually need the slice data.
    /// This specialized helper allocates the backing store for a `str` and ensures alignment.
    #[inline]
    pub fn alloc_str_bytes(&mut self, len: usize) -> *mut u8 {
        // Strings don't need specific alignment > 1, usually, but let's respect general alignment rules.
        let align = 1;
        let offset = self.offset_for_alignment(align);
        let total_size = offset + len;

        self.ensure_capacity(total_size);
        
        let base = self.current_ptr.as_ptr();
        let aligned_ptr = unsafe { base.add(offset) };
        
        self.current_ptr = unsafe { NonNull::new_unchecked(aligned_ptr.add(len)) };
        
        aligned_ptr
    }

    /// Calculates the number of bytes needed to bump the pointer to satisfy `alignment`.
    #[inline]
    fn offset_for_alignment(&self, alignment: usize) -> usize {
        // Current pointer address
        let addr = self.current_ptr.as_ptr() as usize;
        // Calculate misalignment
        let misalignment = addr % alignment;
        // If aligned, 0. Else, (alignment - misalignment)
        if misalignment == 0 { 0 } else { alignment - misalignment }
    }

    /// Clears the arena, resetting all bump pointers but keeping the allocated memory chunks.
    ///
    /// This is a very fast operation as it does not deallocate system memory.
    /// All previously allocated references are invalidated.
    pub fn reset(&mut self) {
        if let Some(start) = self.current_start {
            self.current_ptr = start;
        } else {
            // No memory, nothing to reset.
        }
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

/// A helper struct to manage contiguous string storage and facilitate scanning.
///
/// It holds the raw pointer and length of a string stored within the Arena.
#[repr(C)]
pub struct StrLayout {
    /// Pointer to the start of the string data in the arena.
    pub ptr: *const u8,
    /// Length of the string in bytes.
    pub len: usize,
}

unsafe impl Send for StrLayout {}
unsafe impl Sync for StrLayout {}

impl StrLayout {
    /// Creates a new `StrLayout` from a pointer and length.
    ///
    /// # Safety
    /// `ptr` must be valid for reads of `len` bytes.
    #[inline]
    pub unsafe fn new(ptr

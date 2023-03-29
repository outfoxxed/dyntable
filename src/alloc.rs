//! Stand-in memory allocation types for the ones provided by
//! the `allocator_api` rust unstable feature.

use core::{alloc::Layout, fmt, ptr::NonNull};

/// An implementation of `Deallocator` can deallocate a
/// block of memory allocated in a compatible allocator
/// (usually the type implementing `Deallocator` will also
/// implement `Allocator`)
pub trait Deallocator {
	/// Deallocate a compatible block of memory.
	///
	/// # Safety
	/// The given pointer must be allocated by this allocator,
	/// and representable by the given layout.
	unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: MemoryLayout);
}

/// An implementation of `Allocator` can allocate a block of
/// memory given its layout.
pub trait Allocator: Deallocator {
	/// Allocate a block of memory, given its layout
	///
	/// # Errors
	/// An [`AllocError`] is returned if the allocator cannot
	/// allocate the specified memory block for any reason.
	///
	/// Implementations are encouraged to return Err on memory exhaustion
	/// rather than panicking or aborting, but this is not a strict requirement.
	/// (Specifically: it is legal to implement this trait atop an underlying
	/// native allocation library that aborts on memory exhaustion.)
	fn allocate(&self, layout: MemoryLayout) -> Result<NonNull<[u8]>, AllocError>;
}

/// Layout of a block of memory
///
/// Stand-in for [`core::alloc::Layout`]
#[derive(Copy, Clone)]
#[repr(C)]
pub struct MemoryLayout {
	pub size: usize,
	pub align: usize,
}

impl MemoryLayout {
	/// Construct a new memory layout capable of representing `T`
	pub const fn new<T>() -> Self {
		let layout = Layout::new::<T>();

		Self {
			size: layout.size(),
			align: layout.align(),
		}
	}

	/// Indicates if a memory layout is zero sized, in which case
	/// no memory should actually be allocated
	pub const fn is_zero_sized(&self) -> bool {
		self.size == 0
	}
}

/// The `AllocError` error indicates an allocation failure
/// that may be due to resource exhaustion or to something wrong
/// when combining the given input arguments with this allocator.
///
/// Stand-in for [`std::alloc::AllocError`] (unstable)
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AllocError;

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl std::error::Error for AllocError {}

impl fmt::Display for AllocError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("memory allocation failed")
	}
}

impl From<Layout> for MemoryLayout {
	#[inline(always)]
	fn from(value: Layout) -> Self {
		Self {
			size: value.size(),
			align: value.align(),
		}
	}
}

impl From<MemoryLayout> for Layout {
	#[inline(always)]
	fn from(value: MemoryLayout) -> Self {
		unsafe { Layout::from_size_align_unchecked(value.size, value.align) }
	}
}

#[cfg(all(not(doc), feature = "allocator_api"))]
pub use std_alloc::alloc::Global as GlobalAllocator;
#[cfg(any(doc, not(feature = "allocator_api")))]
/// The global memory allocator
pub struct GlobalAllocator;

#[cfg(all(not(doc), feature = "allocator_api"))]
impl<T: std_alloc::alloc::Allocator> Allocator for T {
	#[inline(always)]
	fn allocate(&self, layout: MemoryLayout) -> Result<NonNull<[u8]>, AllocError> {
		<T as std_alloc::alloc::Allocator>::allocate(self, layout.into()).map_err(|_| AllocError)
	}
}

#[cfg(all(not(doc), feature = "allocator_api"))]
impl<T: std_alloc::alloc::Allocator> Deallocator for T {
	#[inline(always)]
	unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: MemoryLayout) {
		<T as std_alloc::alloc::Allocator>::deallocate(self, ptr, layout.into());
	}
}

#[cfg(any(doc, all(feature = "alloc", not(feature = "allocator_api"))))]
impl Allocator for GlobalAllocator {
	#[inline]
	fn allocate(&self, layout: MemoryLayout) -> Result<NonNull<[u8]>, AllocError> {
		unsafe {
			if layout.is_zero_sized() {
				return Ok(NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
					layout.align as *mut u8,
					0,
				)))
			}

			let memory = std_alloc::alloc::alloc(layout.into());

			match memory.is_null() {
				true => Err(AllocError),
				false => Ok(NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
					memory,
					layout.size,
				))),
			}
		}
	}
}

#[cfg(any(doc, all(feature = "alloc", not(feature = "allocator_api"))))]
impl Deallocator for GlobalAllocator {
	#[inline(always)]
	unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: MemoryLayout) {
		std_alloc::alloc::dealloc(ptr.as_ptr(), layout.into());
	}
}

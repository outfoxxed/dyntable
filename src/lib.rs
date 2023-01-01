#![cfg_attr(feature = "allocator_api", feature(allocator_api))]

use std::{
	alloc::{handle_alloc_error, Layout},
	ffi::c_void,
	marker::PhantomData,
	mem::MaybeUninit,
	ops::{Deref, DerefMut},
	ptr::NonNull,
};

/// Dyntable implementation details. You should not depend on these.
#[doc(hidden)]
#[path = "private.rs"]
pub mod __private;

/// Types with an associated VTable
///
/// # Safety
/// The VTable provided must be compatible with the type this
/// trait is applied to.
pub unsafe trait DynTable<'v, V: 'v + VTable> {
	/// The underlying VTable for the type this trait is applied to
	const VTABLE: V;
	const STATIC_VTABLE: &'v V;
}

/// Marker trait for structs that are VTables
pub unsafe trait VTable {
	/// Additional traits that a `Dyn<VTable>` can implement.
	///
	/// Currently used for Send and Sync.
	type Bounds: ?Sized;
}

/// Trait used to drop objects behind a dyntable.
///
/// Only nessesary for the outermost nested vtable,
/// enables using it in a DynBox.
pub unsafe trait DropTable: VTable {
	/// Drop the underlying type of this VTable, without
	/// deallocating it.
	unsafe fn virtual_drop(&self, instance: *mut c_void);
}

/// VTable types with additional embedded VTable(s).
pub trait SubTable<V: VTable>: VTable {
	/// Gets a reference to an embedded VTable of type `V`
	fn subtable(&self) -> &V;
}

impl<V: VTable> SubTable<V> for V {
	fn subtable(&self) -> &V {
		&self
	}
}

/// Marker for representations of VTables to use in generics
///
/// This allows specifying the type this trait is implemented on
/// in place of its VTable in generics, allowing for a clearer interface.
///
/// Implementations are automatically generated with the [`dyntable`] macro
/// for `dyn Trait`, so `MyTraitVTable` can be used as `dyn MyTrait`.
pub trait VTableRepr {
	type VTable: VTable;
}

/// FFI safe wide pointer to a trait `V`
#[repr(C)]
pub struct Dyn<V: VTableRepr + ?Sized> {
	vtable: *const V::VTable,
	dynptr: *mut c_void,
}

unsafe impl<V: VTableRepr + ?Sized> Send for Dyn<V> where <V::VTable as VTable>::Bounds: Send {}
unsafe impl<V: VTableRepr + ?Sized> Sync for Dyn<V> where <V::VTable as VTable>::Bounds: Sync {}

/// Alternate form of &Dyn used to keep the vtable reference available
#[repr(C)]
pub struct DynRef<'a, V: VTableRepr + ?Sized> {
	r#dyn: Dyn<V>,
	_lt: PhantomData<&'a ()>,
}

/// Alternate form of &mut Dyn used to keep the vtable reference available
#[repr(C)]
pub struct DynRefMut<'a, V: VTableRepr + ?Sized> {
	r#dyn: Dyn<V>,
	_lt: PhantomData<&'a ()>,
}

/// Stand-in for `allocator_api` types.
pub mod alloc {
	use std::{alloc::Layout, ptr::NonNull};

	/// The `DynAllocError` error indicates an allocation failure
	/// that may be due to resource exhaustion or to something wrong
	/// when combining the given input arguments with this allocator.
	///
	/// See [`std::alloc::AllocError`]
	pub struct DynAllocError;
	/// An implementation of `DynAllocator` can allocate and deallocate
	/// arbitrary blocks of data.
	///
	/// See [`std::alloc::Allocator`]
	pub unsafe trait DynAllocator {
		/// Attempts to allocate a block of memory (can be 0 sized)
		fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, DynAllocError>;
		/// Deallocates the memory referenced by `ptr`.
		///
		/// # Safety
		/// * `ptr` must denote a block of memory currently allocated via
		///         via this allocator, and
		/// * `layout` must fit that block of memory
		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
	}

	#[cfg(feature = "allocator_api")]
	unsafe impl<A: std::alloc::Allocator> DynAllocator for A {
		fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, DynAllocError> {
			<A as std::alloc::Allocator>::allocate(&self, layout).map_err(|_| DynAllocError)
		}

		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
			<A as std::alloc::Allocator>::deallocate(&self, ptr, layout)
		}
	}

	pub struct DynGlobalAllocator;
	unsafe impl DynAllocator for DynGlobalAllocator {
		fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, DynAllocError> {
			unsafe {
				Ok(match layout.size() {
					0 => NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
						layout.align() as *mut u8,
						0,
					)),
					size => NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
						std::alloc::alloc(layout),
						size,
					)),
				})
			}
		}

		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
			if layout.size() > 0 {
				std::alloc::dealloc(ptr.as_ptr(), layout)
			}
		}
	}
}

/// Layout used to deallocate a dyn ptr using it's allocator
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DynLayout {
	size: usize,
	align: usize,
}

impl From<Layout> for DynLayout {
	fn from(value: Layout) -> Self {
		Self {
			size: value.size(),
			align: value.align(),
		}
	}
}

impl From<DynLayout> for Layout {
	fn from(value: DynLayout) -> Self {
		unsafe { Layout::from_size_align_unchecked(value.size, value.align) }
	}
}

/// FFI Safe `Box<dyn V>`
///
/// Owned version of [`Dyn`]
#[repr(C)]
pub struct DynBox<V, A = DynGlobalAllocator>
where
	A: DynAllocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	r#dyn: Dyn<V>,
	alloc: A,
	layout: DynLayout,
}

impl<V: VTableRepr + ?Sized> Deref for DynRef<'_, V> {
	type Target = Dyn<V>;

	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V: VTableRepr + ?Sized> Deref for DynRefMut<'_, V> {
	type Target = Dyn<V>;

	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V: VTableRepr + ?Sized> DerefMut for DynRefMut<'_, V> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.r#dyn
	}
}

impl<V> Deref for DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	type Target = Dyn<V>;

	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V> DerefMut for DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.r#dyn
	}
}

impl<V, A> DynBox<V, A>
where
	A: DynAllocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	/// Construct a `DynBox` by moving a value
	/// into the global allocator.
	///
	/// Panics on allocation failure
	pub fn new<'v, T>(data: T) -> DynBox<V, DynGlobalAllocator>
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		DynBox::new_in(data, DynGlobalAllocator)
	}

	/// Construct a `DynBox` by moving a value
	/// into the given allocator.
	///
	/// # Panics
	/// Panics on allocation failure
	pub fn new_in<'v, T>(data: T, alloc: A) -> Self
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		match Self::try_new_in(data, alloc) {
			Ok(dynbox) => dynbox,
			Err(_) => handle_alloc_error(Layout::new::<MaybeUninit<T>>()),
		}
	}

	/// Attempt to construct a `DynBox` by moving a value
	/// into the given allocator.
	pub fn try_new_in<'v, T>(data: T, alloc: A) -> Result<Self, DynAllocError>
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		let layout = Layout::new::<MaybeUninit<T>>();
		let ptr = alloc.allocate(layout)?.cast::<MaybeUninit<T>>();

		unsafe {
			(ptr.as_ptr() as *mut _ as *mut T).write(data);
			Ok(Self::from_raw_in(ptr.as_ptr() as *mut _ as *mut T, alloc))
		}
	}

	/// Constructs a `DynBox` from a raw pointer in the given allocator.
	pub unsafe fn from_raw_in<'v, T>(raw: *mut T, alloc: A) -> Self
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: raw as *mut c_void,
			},
			alloc,
			layout: Layout::new::<T>().into(),
		}
	}

	pub unsafe fn from_raw_dyn_in<'v>(
		ptr: *mut c_void,
		vtable: *const V::VTable,
		alloc: A,
		layout: DynLayout,
	) -> Self
	where
		V::VTable: 'v,
	{
		Self {
			r#dyn: Dyn {
				vtable,
				dynptr: ptr,
			},
			alloc,
			layout,
		}
	}

	pub fn borrow<'s>(&'s self) -> DynRef<'s, V> {
		DynRef {
			r#dyn: Dyn { ..self.r#dyn },
			_lt: PhantomData,
		}
	}

	pub fn borrow_mut<'s>(&'s mut self) -> DynRefMut<'s, V> {
		DynRefMut {
			r#dyn: Dyn { ..self.r#dyn },
			_lt: PhantomData,
		}
	}
}

#[cfg(feature = "allocator_api")]
impl<'v, T, V, A> From<Box<T, A>> for DynBox<V, A>
where
	A: std::alloc::Allocator,
	T: DynTable<'v, V::VTable>,
	V: VTableRepr + ?Sized,
	V::VTable: 'v + DropTable,
{
	fn from(value: Box<T, A>) -> Self {
		let (ptr, alloc) = Box::into_raw_with_allocator(value);
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: ptr as *mut c_void,
			},
			alloc,
			layout: Layout::new::<T>().into(),
		}
	}
}

#[cfg(not(feature = "allocator_api"))]
impl<'v, T, V> From<Box<T>> for DynBox<V, DynGlobalAllocator>
where
	T: DynTable<'v, V::VTable>,
	V: VTableRepr + ?Sized,
	V::VTable: 'v + DropTable,
{
	fn from(value: Box<T>) -> Self {
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				// box uses the same global allocator
				dynptr: Box::into_raw(value) as *mut c_void,
			},
			alloc: DynGlobalAllocator,
			layout: Layout::new::<T>().into(),
		}
	}
}

impl<V, A> Drop for DynBox<V, A>
where
	A: DynAllocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	fn drop(&mut self) {
		unsafe {
			(*self.r#dyn.vtable).virtual_drop(self.r#dyn.dynptr);
			self.alloc.deallocate(
				NonNull::new_unchecked(self.r#dyn.dynptr as *mut u8),
				self.layout.into(),
			);
		}
	}
}

use alloc::{DynAllocError, DynAllocator, DynGlobalAllocator};

pub use dyntable_macro::dyntable;

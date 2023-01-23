#![cfg_attr(feature = "allocator_api", feature(allocator_api))]

use std::{
	alloc::{handle_alloc_error, Layout},
	ffi::c_void,
	marker::PhantomData,
	mem::{self, MaybeUninit},
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
///
/// # Safety
/// `Bounds` must accurately reflect the trait bounds of the trait
/// thid VTable belongs to
pub unsafe trait VTable {
	/// Additional traits that a `Dyn<VTable>` can implement.
	///
	/// Currently used for Send and Sync.
	type Bounds: ?Sized;
}

/// Trait used to drop objects behind a dyntable
///
/// Only nessesary for the outermost nested vtable,
/// enables using it in a DynBox.
pub trait DropTable: VTable {
	/// Drop the underlying type of this VTable, without
	/// deallocating it.
	///
	/// # Safety
	/// This function must drop the underlying value using
	/// its drop function.
	unsafe fn virtual_drop(&self, instance: *mut c_void);
}

/// VTable types with additional embedded VTable(s)
pub trait SubTable<V: VTable>: VTable {
	/// Gets a reference to an embedded VTable of type `V`
	fn subtable(&self) -> &V;
}

impl<V: VTable> SubTable<V> for V {
	#[inline(always)]
	fn subtable(&self) -> &V {
		self
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

/// # Notes
/// This trait is used as an implementation target for
/// [`dyntable`] annotated traits.
///
/// # Safety
/// * The pointer provided by `dyn_ptr` must be valid for
/// at least the lifetime of self and must be compatible
/// with the vtable provided by `dyn_vtable`.
/// * The VTable pointer provided by `dyn_vtable` must be
/// valid for at least the lifetime of self.
pub unsafe trait AsDyn {
	type Repr: VTableRepr + ?Sized;

	/// # Safety
	/// This pointer is valid for at least `'self`.
	fn dyn_ptr(&self) -> *mut c_void;
	/// # Safety
	/// This pointer is valid for at least `'self`.
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable;
	/// Deallocate the contained pointer without dropping
	///
	/// # Note
	/// This function may panic if it is unreasonable to
	/// deallocate the contained pointer.
	fn dyn_dealloc(self);
}

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

	/// An implementation of `Deallocator` can deallocate a
	/// block of memory allocated in a compatible allocator
	/// (usually the type implementing `Deallocator` will also
	/// implement `Allocator`)
	pub trait Deallocator {
		type DeallocLayout: MemoryLayout;
		/// Deallocate a compatible block of memory, given a pointer
		/// to it and associated information about it (usually the
		/// memory layout or `()`)
		///
		/// # Safety
		/// The given pointer must be allocated by this allocator,
		/// and representable by the given layout.
		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Self::DeallocLayout);
	}

	/// An implementation of `Allocator` can allocate a block of
	/// memory given its layout.
	pub trait Allocator: Deallocator {
		type AllocLayout: MemoryLayout;
		/// Allocate a block of memory, given it's layout
		///
		/// # Errors
		/// An [`AllocError`] is returned if the allocator cannot
		/// allocate the specified memory block for any reason.
		fn allocate(&self, layout: Self::AllocLayout) -> Result<NonNull<[u8]>, AllocError>;
	}

	/// Layout of a block of memory
	///
	/// Stand-in for [`core::alloc::Layout`]
	pub trait MemoryLayout: Clone {
		/// Construct a new memory layout capable of representing `T`
		fn new<T>() -> Self;
		/// Indicates if a memory layout is zero sized, in which case
		/// no memory should actually be allocated
		fn is_zero_sized(&self) -> bool;
	}

	#[derive(Clone)]
	#[repr(C)]
	pub struct RustLayout {
		size: usize,
		align: usize,
	}

	impl MemoryLayout for RustLayout {
		fn new<T>() -> Self {
			Layout::new::<T>().into()
		}

		fn is_zero_sized(&self) -> bool {
			self.size == 0
		}
	}

	/// The `AllocError` error indicates an allocation failure
	/// that may be due to resource exhaustion or to something wrong
	/// when combining the given input arguments with this allocator.
	///
	/// Stand-in for [`std::alloc::AllocError`] (unstable)
	pub struct AllocError;

	impl From<Layout> for RustLayout {
		fn from(value: Layout) -> Self {
			Self {
				size: value.size(),
				align: value.align(),
			}
		}
	}

	impl From<RustLayout> for Layout {
		fn from(value: RustLayout) -> Self {
			unsafe { Layout::from_size_align_unchecked(value.size, value.align) }
		}
	}

	/// Rust global allocator
	#[cfg(feature = "allocator_api")]
	pub use std::alloc::Global as GlobalAllocator;
	#[cfg(not(feature = "allocator_api"))]
	pub struct GlobalAllocator;

	#[cfg(feature = "allocator_api")]
	impl<T: std::alloc::Allocator> Allocator for T {
		type AllocLayout = RustLayout;

		fn allocate(&self, layout: RustLayout) -> Result<NonNull<[u8]>, AllocError> {
			<T as std::alloc::Allocator>::allocate(self, layout.into()).map_err(|_| AllocError)
		}
	}

	#[cfg(feature = "allocator_api")]
	impl<T: std::alloc::Allocator> Deallocator for T {
		type DeallocLayout = RustLayout;

		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: RustLayout) {
			<T as std::alloc::Allocator>::deallocate(self, ptr, layout.into());
		}
	}

	#[cfg(not(feature = "allocator_api"))]
	impl Allocator for GlobalAllocator {
		type AllocLayout = RustLayout;

		fn allocate(&self, layout: RustLayout) -> Result<NonNull<[u8]>, AllocError> {
			unsafe {
				Ok(NonNull::new_unchecked(core::ptr::slice_from_raw_parts_mut(
					std::alloc::alloc(layout.into()),
					0,
				)))
			}
		}
	}

	#[cfg(not(feature = "allocator_api"))]
	impl Deallocator for GlobalAllocator {
		type DeallocLayout = RustLayout;

		unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: RustLayout) {
			std::alloc::dealloc(ptr.as_ptr(), layout.into());
		}
	}
}

/// FFI Safe `Box<dyn V>`
///
/// Owned version of [`Dyn`]
#[repr(C)]
pub struct DynBox<V, A = GlobalAllocator>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	r#dyn: Dyn<V>,
	alloc: A,
	layout: A::DeallocLayout,
}

unsafe impl<V: VTableRepr + ?Sized> AsDyn for Dyn<V> {
	type Repr = V;

	#[inline(always)]
	fn dyn_ptr(&self) -> *mut c_void {
		self.dynptr
	}

	#[inline(always)]
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable {
		self.vtable
	}

	fn dyn_dealloc(self) {
		unreachable!("raw dynpointers cannot be deallocated");
	}
}

unsafe impl<V> AsDyn for DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	type Repr = V;

	#[inline(always)]
	fn dyn_ptr(&self) -> *mut c_void {
		self.r#dyn.dynptr
	}

	#[inline(always)]
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable {
		self.r#dyn.vtable
	}

	fn dyn_dealloc(self) {
		unsafe {
			self.alloc.deallocate(
				NonNull::new_unchecked(self.r#dyn.dynptr as *mut _ as *mut u8),
				self.layout.clone(),
			);
		}

		mem::forget(self);
	}
}

impl<V: VTableRepr + ?Sized> Deref for DynRef<'_, V> {
	type Target = Dyn<V>;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V: VTableRepr + ?Sized> Deref for DynRefMut<'_, V> {
	type Target = Dyn<V>;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V: VTableRepr + ?Sized> DerefMut for DynRefMut<'_, V> {
	#[inline(always)]
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

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V> DerefMut for DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	#[inline(always)]
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.r#dyn
	}
}

impl<V, A> DynBox<V, A>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	/// Construct a `DynBox` by moving a value
	/// into the global allocator.
	///
	/// Panics on allocation failure
	pub fn new<'v, T>(data: T) -> DynBox<V, GlobalAllocator>
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		DynBox::new_in(data, GlobalAllocator)
	}

	/// Construct a `DynBox` by moving a value
	/// into the given allocator.
	///
	/// # Panics
	/// Panics on allocation failure
	pub fn new_in<'v, T>(data: T, alloc: A) -> Self
	where
		A: Allocator,
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
	pub fn try_new_in<'v, T>(data: T, alloc: A) -> Result<Self, AllocError>
	where
		A: Allocator,
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		let layout = A::AllocLayout::new::<MaybeUninit<T>>();

		unsafe {
			let ptr = match layout.is_zero_sized() {
				true => NonNull::<MaybeUninit<T>>::dangling(),
				false => {
					let ptr = alloc.allocate(layout)?.cast();
					(ptr.as_ptr() as *mut _ as *mut T).write(data);
					ptr
				},
			};

			Ok(Self::from_raw_in(ptr.as_ptr() as *mut _ as *mut T, alloc))
		}
	}

	/// Constructs a `DynBox` from a raw pointer in the given allocator
	///
	/// # Safety
	/// The pointer `ptr` must be an owned pointer to memory allocated
	/// by the allocator `alloc`.
	pub unsafe fn from_raw_in<'v, T>(ptr: *mut T, alloc: A) -> Self
	where
		T: DynTable<'v, V::VTable>,
		V::VTable: 'v,
	{
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: ptr as *mut c_void,
			},
			alloc,
			layout: A::DeallocLayout::new::<T>(),
		}
	}

	/// Constructs a `DynBox` from a raw pointer and a layout in the
	/// given allocator
	///
	/// # Safety
	/// The pointer `ptr` must be an owned pointer to memory allocated
	/// by the allocator `alloc`. It's memory layout must match the one
	/// descibed by `layout`, and it must be compatible with the vtable `vtable`.
	pub unsafe fn from_raw_dyn_in<'v>(
		ptr: *mut c_void,
		vtable: *const V::VTable,
		alloc: A,
		layout: A::DeallocLayout,
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

	pub fn borrow(&self) -> DynRef<V> {
		DynRef {
			r#dyn: Dyn { ..self.r#dyn },
			_lt: PhantomData,
		}
	}

	pub fn borrow_mut(&mut self) -> DynRefMut<V> {
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
impl<'v, T, V> From<Box<T>> for DynBox<V, GlobalAllocator>
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
			alloc: GlobalAllocator,
			layout: Layout::new::<T>().into(),
		}
	}
}

impl<V, A> Drop for DynBox<V, A>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	fn drop(&mut self) {
		unsafe {
			(*self.r#dyn.vtable).virtual_drop(self.r#dyn.dynptr);

			if !self.layout.is_zero_sized() {
				self.alloc.deallocate(
					NonNull::new_unchecked(self.r#dyn.dynptr as *mut u8),
					self.layout.clone(),
				);
			}
		}
	}
}

use alloc::{AllocError, Allocator, Deallocator, GlobalAllocator, MemoryLayout};

pub use dyntable_macro::dyntable;

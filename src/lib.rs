//!  -- TODO --
//!
//! # Basic Usage
//! ```
//! use dyntable::*;
//!
//! #[dyntable]
//! trait Animal {
//!     extern "C" fn eat_food(&mut self, amount: u32);
//!     extern "C" fn is_full(&self) -> bool;
//! }
//!
//! struct Dog {
//!     food_count: u32,
//! }
//!
//! struct Cat {
//!     picky: bool,
//!     food_count: u32,
//! }
//!
//! impl Animal for Dog {
//!     extern "C" fn eat_food(&mut self, amount: u32) {
//!         self.food_count += amount;
//!     }
//!
//!     extern "C" fn is_full(&self) -> bool {
//!         self.food_count > 200
//!     }
//! }
//!
//! # // stub
//! # fn random_chance() -> bool { true }
//!
//! impl Animal for Cat {
//!     extern "C" fn eat_food(&mut self, amount: u32) {
//!         if !self.picky || random_chance() {
//!             self.food_count += amount;
//!         }
//!     }
//!
//!     extern "C" fn is_full(&self) -> bool {
//!         self.food_count > 100
//!     }
//! }
//!
//! fn main() {
//!    let mut pets = [
//!        DynBox::<dyn Animal>::new(Dog {
//!            food_count: 10,
//!        }),
//!        DynBox::<dyn Animal>::new(Cat {
//!            picky: true,
//!            food_count: 30,
//!        }),
//!        DynBox::<dyn Animal>::new(Cat {
//!            picky: false,
//!            food_count: 0,
//!        }),
//!    ];
//!
//!    // feed all the pets until they are full
//!    loop {
//!        let mut all_full = true;
//!
//!        // feed all the pets
//!        for pet in &mut pets {
//!            if !pet.is_full() {
//!                pet.eat_food(10);
//!                all_full = false;
//!            }
//!        }
//!
//!        // if all pets have finished eating we are done.
//!        if all_full { break; }
//!    }
//! }
//! ```
//!
//! # Crate Features
//! - `allocator_api` - enable support for the unstable `allocator_api` stdlib feature

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
pub mod alloc;

/// This trait provides an instance of the given VTable matching this
/// type.
///
/// # Safety
/// The VTable provided must be compatible with the type this
/// trait is applied to.
///
/// # Notes
/// This trait is implemented by the [`dyntable`] macro.
pub unsafe trait DynTrait<'v, V: 'v + VTable> {
	/// The underlying VTable for the type this trait is applied to.
	const VTABLE: V;
	/// An instance of the `VTABLE` constant.
	const STATIC_VTABLE: &'v V;
}

/// This trait indicates that the target type is the VTable.
///
/// # Safety
/// The assoicated `Bounds` type must accurately reflect the
/// trait bounds of the trait this VTable belongs to.
pub unsafe trait VTable {
	/// Additional trait bounds required by the trait this VTable
	/// belongs to.
	///
	/// Bounds are intended to be specified as a `dyn <trait list>`.
	///
	/// # Notes
	/// This trait is implemented by the [`dyntable`] macro.
	///
	/// This trait is used to apply [`Send`] and [`Sync`] to
	/// dyntable containers.
	type Bounds: ?Sized;
}

/// Trait providing a drop function for a given opaque pointer.
///
/// An implementation of this trait (when combined with [`AssociatedLayout`])
/// allows a type associated with this VTable to be used in an owning
/// dyn container such as a [`DynBox`] in addition to non-owning dyntable
/// containers such as [`DynRef`] which can be used without `AssociatedDrop`.
///
/// # Safety
/// `vitrual_drop` must drop the given pointer as if it was a
/// pointer to a type associated with this VTable.
///
/// # Notes
/// This trait is implemented by the [`dyntable`] macro.
pub unsafe trait AssociatedDrop: VTable {
	/// Drop the given pointer as if it is a pointer to a
	/// type associated with this VTable, without deallocating
	/// the given pointer.
	///
	/// # Safety
	/// The pointer must point to a valid instance of a type
	/// this VTable is able to drop. After calling this function
	/// the instance pointed to by this pointer must be considered
	/// to be dropped.
	unsafe fn virtual_drop(&self, instance: *mut c_void);
}

/// Trait providing a function to look up the layout of the associated type.
///
/// An implementation of this trait (when combined with [`AssociatedDrop`])
/// allows a type associated with this VTable to be used in an owning
/// dyn container such as a [`DynBox`] in addition to non-owning dyntable
/// containers such as [`DynRef`] which can be used without `AssociatedLayout`.
///
/// # Safety
/// `vitrtual_layout` must return the correct layout for the associated type.
///
/// # Notes
/// This trait is implemented by the [`dyntable`] macro.
pub unsafe trait AssociatedLayout: VTable {
	/// Get the layout matching the associated type.
	fn virtual_layout(&self) -> MemoryLayout;
}

/// This trait describes this VTable as containing another
/// VTable.
///
/// # Notes
/// This trait is implemented by the [`dyntable`] macro.
///
/// This trait is used along with an implementation of [`AsDyn`]
/// to allow calling functions associated with bounded traits
/// of this VTable's associated trait.
pub trait SubTable<V: VTable>: VTable {
	/// Returns a reference to the contained VTable of type `V`.
	fn subtable(&self) -> &V;
}

impl<V: VTable> SubTable<V> for V {
	#[inline(always)]
	fn subtable(&self) -> &V {
		self
	}
}

/// Marker for representations of VTables to use in generics.
///
/// This allows specifying the type this trait is implemented on
/// in place of its VTable in generics, allowing for a clearer interface.
///
/// # Notes
/// This trait is implemented by the [`dyntable`] macro.
/// for `dyn Trait`, so `MyTraitVTable` can be used as `dyn MyTrait`.
pub trait VTableRepr {
	type VTable: VTable;
}

/// An FFI safe wide pointer to a dyntable trait, AKA dynptr.
///
/// Use [`DynRef::from_raw`] or [`DynRefMut::from_raw`] to call functions
/// on this pointer.
///
/// # Notes
/// While constructing a [`DynPtr`] is always safe, using the pointer
/// is only safe as long as the `ptr` and `vtable` fields both point to
/// valid data.
#[repr(C)]
pub struct DynPtr<V: VTableRepr + ?Sized> {
	// Having the data pointer before the VTable pointer generates
	// better ASM. (the compiler cannot change layout due to #[repr(C)])
	pub ptr: *mut c_void,
	pub vtable: *const V::VTable,
}

impl<V: VTableRepr + ?Sized> Copy for DynPtr<V> {}
impl<V: VTableRepr + ?Sized> Clone for DynPtr<V> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<V: VTableRepr + ?Sized> DynPtr<V> {
	/// Create a [`DynPtr`] to the same data as the given pointer.
	///
	/// This method uses the static VTable associated with the provided
	/// type. To use a different VTable, construct the [`DynPtr`] manually.
	#[inline(always)]
	pub fn new<'v, T>(ptr: *mut T) -> Self
	where
		T: DynTrait<'v, V::VTable>,
		V::VTable: 'v,
	{
		Self {
			ptr: ptr as *mut c_void,
			vtable: T::STATIC_VTABLE,
		}
	}

	/// Upcast the given dynptr to a bounded dyntrait ptr.
	///
	/// This pointer may still be used after upcasting it, in addition
	/// to the returned (upcasted) pointer.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait Animal {}
	///
	/// #[dyntable]
	/// trait Feline: Animal
	/// where
	///     dyn Animal:,
	/// {}
	///
	/// struct Cat;
	///
	/// impl Feline for Cat {}
	/// impl Animal for Cat {}
	///
	/// let cat: DynPtr<dyn Feline> = DynBox::into_raw(DynBox::new(Cat));
	/// let feline: DynPtr<dyn Animal> = DynPtr::upcast(cat);
	/// // Move the pointer back into a box to drop it.
	/// let _ = unsafe { DynBox::from_raw_in(feline, dyntable::alloc::GlobalAllocator) };
	/// ```
	#[inline(always)]
	pub fn upcast<U>(ptr: Self) -> DynPtr<U>
	where
		U: VTableRepr + ?Sized,
		V::VTable: SubTable<U::VTable>,
	{
		DynPtr {
			ptr: ptr.ptr,
			/// SAFETY: the subtable is a slice into the existing vtable
			/// pointer, and therefore has the same lifetime.
			vtable: unsafe { (*ptr.vtable).subtable() },
		}
	}
}

/// Wrapper around a raw DynPtr for implementing abstractions.
///
/// Implements Send and Sync when they are bounds of the target trait.
#[repr(transparent)]
pub struct DynUnchecked<V: VTableRepr + ?Sized> {
	pub ptr: DynPtr<V>,
}

impl<V: VTableRepr + ?Sized> Copy for DynUnchecked<V> {}
impl<V: VTableRepr + ?Sized> Clone for DynUnchecked<V> {
	fn clone(&self) -> Self {
		*self
	}
}

unsafe impl<V: VTableRepr + ?Sized> Send for DynUnchecked<V> where
	<V::VTable as VTable>::Bounds: Send
{
}
unsafe impl<V: VTableRepr + ?Sized> Sync for DynUnchecked<V> where
	<V::VTable as VTable>::Bounds: Sync
{
}

/// This trait implements the trait described by its `Repr`.
///
/// # Safety
/// The pointer provided by `dyn_ptr` must be valid for
/// at least the lifetime of self and must be compatible
/// with the vtable provided by `dyn_vtable`.
///
/// The VTable pointer provided by `dyn_vtable` must be
/// valid for at least the lifetime of self.
///
/// # Notes
/// This trait is used to implement dyntable container types.
pub unsafe trait AsDyn {
	/// The dyn Trait that will be implemented for this type
	type Repr: VTableRepr + ?Sized;

	/// Returns a pointer to the underlying data of this dynptr.
	///
	/// The provided pointer will be valid for at least the lifetime
	/// of this value.
	fn dyn_ptr(&self) -> *mut c_void;
	/// Returns a pointer to the vtable used to interact with the pointer
	/// provided by `dyn_ptr`.
	///
	/// The provided pointer will be valid for at least the lifetime
	/// of this value.
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable;
	/// Deallocate the contained pointer without dropping
	///
	/// This function may panic if it is unreasonable to
	/// deallocate the contained pointer. Such cases include
	/// deallocating a `DynRefCallProxy`, which is a proxy type
	/// that cannot be obtained except behind a reference.
	fn dyn_dealloc(self);
}

/// Reference to a dyntable Trait, equivalent to `&dyn Trait`.
#[repr(transparent)]
pub struct DynRef<'a, V: VTableRepr + ?Sized> {
	ptr: DynPtr<V>,
	_lt: PhantomData<&'a ()>,
}

impl<V: VTableRepr + ?Sized> Copy for DynRef<'_, V> {}
impl<V: VTableRepr + ?Sized> Clone for DynRef<'_, V> {
	#[inline(always)]
	fn clone(&self) -> Self {
		*self
	}
}

unsafe impl<V: VTableRepr + ?Sized> Send for DynRef<'_, V> where <V::VTable as VTable>::Bounds: Sync {}

impl<'a, V: VTableRepr + ?Sized> DynRef<'a, V> {
	/// Casts a [`DynPtr`] to a [`DynRef`].
	///
	/// # Safety
	/// The pointer `ptr` must be an owned dynptr to memory allocated
	/// by the rust global allocator. It's data pointer and vtable pointer
	/// must both live for the provided lifetime of the created reference.
	///
	/// # Examples
	/// Use a [`DynRef`] to call a function on a [`DynPtr`]:
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {
	///     extern "C" fn foo(&self);
	/// }
	///
	/// struct MyStruct;
	///
	/// impl MyTrait for MyStruct {
	///     extern "C" fn foo(&self) {}
	/// }
	///
	/// // leak a dynbox into a raw ptr
	/// let x: DynBox<dyn MyTrait> = DynBox::new(MyStruct);
	/// let ptr = DynBox::into_raw(x);
	///
	/// unsafe { DynRef::from_raw(ptr).foo() };
	///
	/// // raw ptr is dropped using a `DynBox`
	/// let _: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw(ptr) };
	/// ```
	#[inline(always)]
	pub unsafe fn from_raw(ptr: DynPtr<V>) -> Self {
		Self {
			ptr,
			_lt: PhantomData,
		}
	}

	#[inline(always)]
	pub fn borrow(r: Self) -> Self {
		r
	}

	/// Upcast the given dynref to a bounded dyntrait ref.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait Animal {}
	///
	/// #[dyntable]
	/// trait Feline: Animal
	/// where
	///     dyn Animal:,
	/// {}
	///
	/// struct Cat;
	///
	/// impl Feline for Cat {}
	/// impl Animal for Cat {}
	///
	/// let feline: DynBox<dyn Feline> = DynBox::new(Cat);
	/// let feline_ref: DynRef<dyn Feline> = DynBox::borrow(&feline);
	/// let animal_ref: DynRef<dyn Animal> = DynRef::upcast(feline_ref);
	/// ```
	#[inline(always)]
	pub fn upcast<U>(r: Self) -> DynRef<'a, U>
	where
		U: VTableRepr + ?Sized,
		V::VTable: SubTable<U::VTable>,
	{
		unsafe { DynRef::from_raw(DynPtr::upcast(r.ptr)) }
	}
}

/// Reference to a dyntable Trait, equivalent to `&mut dyn Trait`.
#[repr(transparent)]
pub struct DynRefMut<'a, V: VTableRepr + ?Sized> {
	ptr: DynPtr<V>,
	_lt: PhantomData<&'a mut ()>,
}

unsafe impl<V: VTableRepr + ?Sized> Send for DynRefMut<'_, V> where
	<V::VTable as VTable>::Bounds: Sync
{
}

impl<'a, V: VTableRepr + ?Sized> Deref for DynRef<'a, V> {
	type Target = DynRefCallProxy<'a, V>;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		DynRefCallProxy::from_raw(&self.ptr)
	}
}

impl<'a, V: VTableRepr + ?Sized> DynRefMut<'a, V> {
	/// Casts a [`DynPtr`] to a [`DynRefMut`].
	///
	/// # Safety
	/// The pointer `ptr` must be an owned dynptr to memory allocated
	/// by the rust global allocator. It's data pointer and vtable pointer
	/// must both live for the provided lifetime of the created reference.
	///
	/// # Examples
	/// Use a [`DynRefMut`] to call a function on a [`DynPtr`]:
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {
	///     extern "C" fn foo(&mut self);
	/// }
	///
	/// struct MyStruct;
	///
	/// impl MyTrait for MyStruct {
	///     extern "C" fn foo(&mut self) {}
	/// }
	///
	/// // leak a dynbox into a raw ptr
	/// let x: DynBox<dyn MyTrait> = DynBox::new(MyStruct);
	/// let ptr = DynBox::into_raw(x);
	///
	/// unsafe { DynRefMut::from_raw(ptr).foo() };
	///
	/// // raw ptr is dropped using a `DynBox`
	/// let _: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw(ptr) };
	/// ```
	#[inline(always)]
	pub unsafe fn from_raw(ptr: DynPtr<V>) -> Self {
		Self {
			ptr,
			_lt: PhantomData,
		}
	}

	#[inline(always)]
	pub fn borrow(b: &Self) -> DynRef<V> {
		unsafe { DynRef::from_raw(b.ptr) }
	}

	#[inline(always)]
	pub fn borrow_mut(r: &mut Self) -> DynRefMut<V> {
		unsafe { DynRefMut::from_raw(r.ptr) }
	}

	/// Upcast the given mutable dynref to a bounded dyntrait ref.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait Animal {}
	///
	/// #[dyntable]
	/// trait Feline: Animal
	/// where
	///     dyn Animal:,
	/// {}
	///
	/// struct Cat;
	///
	/// impl Feline for Cat {}
	/// impl Animal for Cat {}
	///
	/// let mut feline: DynBox<dyn Feline> = DynBox::new(Cat);
	/// let feline_ref: DynRefMut<dyn Feline> = DynBox::borrow_mut(&mut feline);
	/// let animal_ref: DynRefMut<dyn Animal> = DynRefMut::upcast(feline_ref);
	/// ```
	#[inline(always)]
	pub fn upcast<U>(r: Self) -> DynRefMut<'a, U>
	where
		U: VTableRepr + ?Sized,
		V::VTable: SubTable<U::VTable>,
	{
		unsafe { DynRefMut::from_raw(DynPtr::upcast(r.ptr)) }
	}
}

impl<'a, V: VTableRepr + ?Sized> Deref for DynRefMut<'a, V> {
	type Target = DynRefCallProxy<'a, V>;

	#[inline(always)]
	fn deref(&self) -> &Self::Target {
		DynRefCallProxy::from_raw(&self.ptr)
	}
}

impl<V: VTableRepr + ?Sized> DerefMut for DynRefMut<'_, V> {
	#[inline(always)]
	fn deref_mut(&mut self) -> &mut Self::Target {
		DynRefCallProxy::from_raw_mut(&mut self.ptr)
	}
}

#[doc(hidden)]
#[repr(transparent)]
pub struct DynRefCallProxy<'a, V: VTableRepr + ?Sized> {
	ptr: DynPtr<V>,
	_lt: PhantomData<&'a ()>,
}

impl<V: VTableRepr + ?Sized> DynRefCallProxy<'_, V> {
	#[inline(always)]
	fn from_raw(ptr: &DynPtr<V>) -> &Self {
		unsafe { mem::transmute(ptr) }
	}

	#[inline(always)]
	fn from_raw_mut(ptr: &mut DynPtr<V>) -> &mut Self {
		unsafe { mem::transmute(ptr) }
	}
}

unsafe impl<V: VTableRepr + ?Sized> AsDyn for DynRefCallProxy<'_, V> {
	type Repr = V;

	#[inline(always)]
	fn dyn_ptr(&self) -> *mut c_void {
		self.ptr.ptr
	}

	#[inline(always)]
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable {
		self.ptr.vtable
	}

	fn dyn_dealloc(self) {
		unreachable!("references cannot be deallocated");
	}
}

/// An FFI safe Box that operates on dyntable traits.
#[repr(C)]
pub struct DynBox<V, A = GlobalAllocator>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: AssociatedDrop + AssociatedLayout,
{
	alloc: A,
	ptr: DynUnchecked<V>,
}

unsafe impl<V, A> AsDyn for DynBox<V, A>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: AssociatedDrop + AssociatedLayout,
{
	type Repr = V;

	#[inline(always)]
	fn dyn_ptr(&self) -> *mut c_void {
		self.ptr.ptr.ptr
	}

	#[inline(always)]
	fn dyn_vtable(&self) -> *const <Self::Repr as VTableRepr>::VTable {
		self.ptr.ptr.vtable
	}

	fn dyn_dealloc(self) {
		unsafe {
			self.alloc.deallocate(
				NonNull::new_unchecked(self.ptr.ptr.ptr as *mut _ as *mut u8),
				(*self.ptr.ptr.vtable).virtual_layout(),
			);
		}

		mem::forget(self);
	}
}

impl<V> DynBox<V, GlobalAllocator>
where
	V: VTableRepr + ?Sized,
	V::VTable: AssociatedDrop + AssociatedLayout,
{
	/// Allocates memory using the global allocator and moves `data` into
	/// the allocated memory, upcasting it to `V`.
	///
	/// # Panics
	/// This method panics on allocation failure.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new(MyStruct);
	/// ```
	#[inline]
	pub fn new<'v, T>(data: T) -> Self
	where
		T: DynTrait<'v, V::VTable>,
		V::VTable: 'v,
	{
		DynBox::new_in(data, GlobalAllocator)
	}

	/// Constructs a `DynBox` from a raw dynptr in the global allocator.
	///
	/// After calling this function, the raw dynptr is considered to be
	/// owned by the `DynBox` and will be cleaned up as such.
	///
	/// # Safety
	/// The pointer `ptr` must be an owned dynptr to memory allocated
	/// by the rust global allocator.
	///
	/// # Examples
	/// Recreate a `DynBox` which was previously converted to a raw pointer
	/// using [`DynBox::into_raw`]:
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new(MyStruct);
	/// let ptr = DynBox::into_raw(x);
	/// let x: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw(ptr) };
	/// ```
	#[inline(always)]
	pub unsafe fn from_raw(ptr: DynPtr<V>) -> Self {
		Self::from_raw_in(ptr, GlobalAllocator)
	}
}

impl<V, A> DynBox<V, A>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: AssociatedDrop + AssociatedLayout,
{
	/// Allocates memory using the given allocator and moves `data` into
	/// the allocated memory, upcasting it to `V`.
	///
	/// # Panics
	/// This method panics on allocation failure.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new_in(MyStruct, dyntable::alloc::GlobalAllocator);
	/// ```
	#[inline]
	pub fn new_in<'v, T>(data: T, alloc: A) -> Self
	where
		A: Allocator,
		T: DynTrait<'v, V::VTable>,
		V::VTable: 'v,
	{
		match Self::try_new_in(data, alloc) {
			Ok(dynbox) => dynbox,
			Err(_) => handle_alloc_error(Layout::new::<MaybeUninit<T>>()),
		}
	}

	/// Allocates memory using the given allocator and moves `data` into
	/// the allocated memory, upcasting it to `V`, and returning an error
	/// if the allocation fails.
	///
	/// # Panics
	/// This method panics on allocation failure.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::try_new_in(MyStruct, dyntable::alloc::GlobalAllocator)?;
	/// # Ok::<_, dyntable::alloc::AllocError>(())
	/// ```
	#[inline]
	pub fn try_new_in<'v, T>(data: T, alloc: A) -> Result<Self, AllocError>
	where
		A: Allocator,
		T: DynTrait<'v, V::VTable>,
		V::VTable: 'v,
	{
		let layout = MemoryLayout::new::<MaybeUninit<T>>();

		unsafe {
			let ptr = match layout.is_zero_sized() {
				true => NonNull::<MaybeUninit<T>>::dangling(),
				false => {
					let ptr = alloc.allocate(layout)?.cast();
					(ptr.as_ptr() as *mut _ as *mut T).write(data);
					ptr
				},
			};

			Ok(Self::from_raw_in(
				DynPtr::new(ptr.as_ptr() as *mut _ as *mut T),
				alloc,
			))
		}
	}

	/// Constructs a `DynBox` from a raw dynptr in the given allocator.
	///
	/// After calling this function, the raw dynptr is considered to be
	/// owned by the `DynBox` and will be cleaned up as such.
	///
	/// # Safety
	/// The pointer `ptr` must be an owned dynptr to memory allocated
	/// by the allocator `alloc`.
	///
	/// # Examples
	/// Recreate a `DynBox` which was previously converted to a raw pointer
	/// using [`DynBox::into_raw_with_allocator`]:
	///
	/// ```
	/// # use dyntable::*;
	/// use dyntable::alloc::GlobalAllocator;
	///
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new_in(MyStruct, GlobalAllocator);
	/// let (ptr, alloc) = DynBox::into_raw_with_allocator(x);
	/// let x: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw_in(ptr, alloc) };
	/// ```
	#[inline(always)]
	pub unsafe fn from_raw_in(ptr: DynPtr<V>, alloc: A) -> Self {
		Self {
			ptr: DynUnchecked { ptr },
			alloc,
		}
	}

	/// Upcast the dynbox to a bounded dyntrait box.
	///
	/// # Examples
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait Animal {}
	///
	/// #[dyntable]
	/// trait Feline: Animal
	/// where
	///     dyn Animal:,
	/// {}
	///
	/// struct Cat;
	///
	/// impl Feline for Cat {}
	/// impl Animal for Cat {}
	///
	/// let feline: DynBox<dyn Feline> = DynBox::new(Cat);
	/// let animal_ref: DynBox<dyn Animal> = DynBox::upcast(feline);
	/// ```
	#[inline(always)]
	pub fn upcast<U>(b: Self) -> DynBox<U, A>
	where
		U: VTableRepr + ?Sized,
		U::VTable: AssociatedDrop + AssociatedLayout,
		V::VTable: SubTable<U::VTable>,
	{
		let (ptr, alloc) = Self::into_raw_with_allocator(b);
		unsafe { DynBox::from_raw_in(DynPtr::upcast(ptr), alloc) }
	}

	/// Leak a DynBox, returning its DynPtr and Allocator
	///
	/// # Examples
	/// Recreate a `DynBox` which was previously converted to a raw pointer.
	///
	/// ```
	/// # use dyntable::*;
	/// use dyntable::alloc::GlobalAllocator;
	///
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new_in(MyStruct, GlobalAllocator);
	/// let (ptr, alloc) = DynBox::into_raw_with_allocator(x);
	/// let x: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw_in(ptr, alloc) };
	/// ```
	#[inline(always)]
	pub fn into_raw_with_allocator(b: Self) -> (DynPtr<V>, A) {
		// SAFETY: the original value is forgotten
		let alloc = unsafe { (&b.alloc as *const A).read() };
		let ptr = b.ptr.ptr;

		mem::forget(b);

		(ptr, alloc)
	}

	/// Leak a DynBox into a DynPtr
	///
	/// # Examples
	/// Recreate a `DynBox` which was previously converted to a raw pointer.
	///
	/// ```
	/// # use dyntable::*;
	/// #[dyntable]
	/// trait MyTrait {}
	/// struct MyStruct;
	/// impl MyTrait for MyStruct {}
	///
	/// let x: DynBox<dyn MyTrait> = DynBox::new(MyStruct);
	/// let ptr = DynBox::into_raw(x);
	/// let x: DynBox<dyn MyTrait> = unsafe { DynBox::from_raw(ptr) };
	/// ```
	#[inline(always)]
	pub fn into_raw(b: Self) -> DynPtr<V> {
		Self::into_raw_with_allocator(b).0
	}

	/// Immutably borrows the wrapped value.
	#[inline(always)]
	pub fn borrow(b: &Self) -> DynRef<V> {
		DynRef {
			ptr: b.ptr.ptr,
			_lt: PhantomData,
		}
	}

	/// Mutably borrows the wrapped value.
	#[inline(always)]
	pub fn borrow_mut(b: &mut Self) -> DynRefMut<V> {
		DynRefMut {
			ptr: b.ptr.ptr,
			_lt: PhantomData,
		}
	}
}

#[cfg(feature = "allocator_api")]
impl<'v, T, V, A> From<Box<T, A>> for DynBox<V, A>
where
	A: std::alloc::Allocator,
	T: DynTrait<'v, V::VTable>,
	V: VTableRepr + ?Sized,
	V::VTable: 'v + AssociatedDrop + AssociatedLayout,
{
	fn from(value: Box<T, A>) -> Self {
		let (ptr, alloc) = Box::into_raw_with_allocator(value);
		unsafe { Self::from_raw_in(DynPtr::new(ptr), alloc) }
	}
}

#[cfg(not(feature = "allocator_api"))]
impl<'v, T, V> From<Box<T>> for DynBox<V, GlobalAllocator>
where
	T: DynTrait<'v, V::VTable>,
	V: VTableRepr + ?Sized,
	V::VTable: 'v + AssociatedDrop + AssociatedLayout,
{
	fn from(value: Box<T>) -> Self {
		unsafe { Self::from_raw_in(DynPtr::new(Box::into_raw(value)), GlobalAllocator) }
	}
}

impl<V, A> Drop for DynBox<V, A>
where
	A: Deallocator,
	V: VTableRepr + ?Sized,
	V::VTable: AssociatedDrop + AssociatedLayout,
{
	fn drop(&mut self) {
		unsafe {
			let vtable = &*self.ptr.ptr.vtable;
			vtable.virtual_drop(self.ptr.ptr.ptr);

			let layout = vtable.virtual_layout();
			if !layout.is_zero_sized() {
				self.alloc
					.deallocate(NonNull::new_unchecked(self.ptr.ptr.ptr as *mut u8), layout);
			}
		}
	}
}

use alloc::{AllocError, Allocator, Deallocator, GlobalAllocator, MemoryLayout};

/// This macro implements functionality required to use the
/// annotated trait as a FFI safe dynptr.
///
/// When applied to a trait, this macro will generate
/// - A VTable representing the trait, including its bounds and methods.
///   (see [VTable Representation](#vtable-representation))
/// - Implementations of [`VTableRepr`], which provides a path
///   to vtables associated with the trait.
/// - An implementation of the trait for all types implementing
///   [`AsDyn`]`<Repr = (your trait)>`, such as [`DynRef`]`<dyn (your trait)>`.
/// - Various boilerplate used in the above implementations.
///
/// # Trait Requirements
/// - The trait must only contain methods (associated functions and const
///   values are not yet supported)
/// - All trait methods must explicitly specify their ABI, usually `C`, unless
///   the `relax_abi = true` parameter is passed to the `#[dyntable]` invocation
/// - No trait methods may have a receiver type other than `Self`, and must use
///   the explicit self shorthand (`fn foo(&self)`)
/// - The trait must be [object safe][ref-obj-safety].
/// - All trait bounds (supertraits) must also be `#[dyntable]` annotated traits
///   (except `Send` and `Sync`)
/// - All trait bounds, including indirect bounds
///   [must have a specified path](#trait-bound-paths).
///
/// ## Trait Bound Paths
/// All trait bounds (supertraits), except `Send` and `Sync` must be annotated with
/// `#[dyntable]`, and must be marked as such using the `dyn` keyword. This is
/// required for `dyntable` to track indirect trait bounds.
/// Below is an example:
///
/// ```
/// # use dyntable::dyntable;
/// #[dyntable]
/// trait Animal {}
///
/// // `Animal` is a `#[dyntable]` trait and must be explicitly marked as such.
/// #[dyntable]
/// trait Dog: Animal
/// where
///     dyn Animal:,
/// {}
///
/// // `Send` is not a `#[dyntable]` annotated trait, and it is an error
/// // to mark it as such.
/// #[dyntable]
/// trait SendDog: Send + Animal
/// where
///     dyn Animal:,
/// {}
/// ```
///
/// Indirect trait bounds must also be specified to provide a path to the
/// indirect trait bound's VTable from the target trait:
///
/// ```
/// # use dyntable::dyntable;
/// #[dyntable]
/// trait Container {}
///
/// #[dyntable]
/// trait FluidContainer: Container
/// where
///    dyn Container:,
/// {}
///
/// #[dyntable]
/// trait ConsumableContainer: Container
/// where
///     dyn Container:,
/// {}
///
/// #[dyntable]
/// trait Bottle: FluidContainer + ConsumableContainer
/// where
///     // The path to `Container` must be specified.
///     dyn FluidContainer: Container,
///     // Although it does not matter which path is used,
///     // specifying it more than once is an error.
///     dyn ConsumableContainer:,
/// {}
/// ```
///
/// ### Dyn bounds in where clause
/// Rust already defines `dyn Trait` bounds in the `where` clause. However
/// since they are not commonly used (and are even less likely to be used
/// for dyntable traits) dyntable hijacks this syntax. To add a normal
/// `dyn Trait` bound to a dyntable trait, wrap it in parenthesis as shown
/// below.
///
/// ```
/// # use dyntable::dyntable;
/// # #[dyntable]
/// # trait BoundedType {}
/// #[dyntable]
/// trait UsesDyntableBound: BoundedType
/// where
///     // This bound is used by dyntable to describe a bound path.
///     dyn BoundedType:,
/// {}
///
/// #[dyntable]
/// trait UsesRustBound
/// where
///     // This bound is skipped by dyntable and passed directly to rust.
///     (dyn BoundedType):,
/// {}
/// ```
///
/// # Macro Options
/// - `repr` - The generated VTable's repr. `Rust` may be specified in addition
///            to any repr permitted by the `#[repr(...)]` attribute.
///            Defaults to `C`.
/// - `relax_abi` - Relax the requirement that all methods must explicitly
///                 specify their ABI. This restriction is in place to avoid
///                 accidentally creating functions with the `Rust` ABI when
///                 you want a FFI compatible abi, usually `C`, which is
///                 dyntable's intended use case.
///                 Defaults to `false`.
/// - `drop` - Specify the existence and ABI of the VTable's `drop` function.
///            Valid options are `none`, to remove the `drop` function, or
///            any ABI permitted by the `extern "..."` specifier.
///            Required to use this trait in owning dyn containers such as [`DynBox`]
///            Defaults to `C`.
/// - `embed_layout` - Embed the layout (size + align) of the implementing type
///                    in the vtable.
///                    Required to use this trait in owning dyn containers such as [`DynBox`]
///                    Defaults to `true`.
/// - `vtable` - Specify the name of the generated VTable.
///              Defaults to `(your trait)VTable`.
///
/// All above options are optional. Below is an example of the `#[dyntable]`
/// attribute with all options explicitly specified with default values:
/// ```
/// # use dyntable::dyntable;
/// #[dyntable(repr = C, relax_abi = false, drop = C, vtable = MyTraitVTable)]
/// trait MyTrait {}
/// ```
///
/// # VTable Layout
/// VTables are represented as a struct that is by default `#[repr(C)]` (see
/// the `repr` option described in [Macro Options](#macro-options)).
/// The VTable entries are laid out in the order they have been listed in,
/// preceeded by a pointer to the type's `drop` function, the memory layout
/// of the trait's implementing type (if not disabled) and any `dyn` bounds
/// (in the order they appear) as shown below:
///
/// ```
/// # use dyntable::*;
/// #[dyntable]
/// trait BoundOfBound {}
///
/// #[dyntable]
/// trait BoundOfMyTrait1: BoundOfBound
/// where
///     dyn BoundOfBound:,
/// {}
///
/// #[dyntable]
/// trait BoundOfMyTrait2 {}
///
/// #[dyntable]
/// trait MyTrait: BoundOfMyTrait1 + BoundOfMyTrait2
/// where
///     dyn BoundOfMyTrait1: BoundOfBound,
///     dyn BoundOfMyTrait2:,
/// {
///     extern "C" fn my_function1(&self);
///     extern "C" fn my_function2(&self);
/// }
///
/// // MyTrait's VTable:
///
/// #[repr(C)]
/// struct VTableForMyTrait {
///     // drop and layout come first if enabled
///     drop: unsafe extern "C" fn(*mut core::ffi::c_void),
///     layout: dyntable::alloc::MemoryLayout,
///     // any bounded dyntable trait VTables follow
///     bound1_vtable: <dyn BoundOfMyTrait1 as VTableRepr>::VTable,
///     // note that BoundOfBound does not have an entry, as it is contained
///     // in BoundOfMyTrait's VTable.
///     bound2_vtable: <dyn BoundOfMyTrait2 as VTableRepr>::VTable,
///     // any member functions follow
///     my_function1: extern "C" fn(*const core::ffi::c_void),
///     my_function2: extern "C" fn(*const core::ffi::c_void),
/// }
/// # // this sanity check is at least better than nothing
/// # use std::alloc::Layout;
/// # assert_eq!(Layout::new::<VTableForMyTrait>(), Layout::new::<<dyn MyTrait as VTableRepr>::VTable>());
/// ```
///
/// ## Backwards Compatibility
/// VTables are fully backwards compatible, as long as:
/// - The VTables of all trait bounds are backwards compatible.
/// - The order of dyn entries for trait bounds must match previous versions.
/// - The paths given to multilevel trait bounds must match.
///   `where dyn A: C, dyn B` is not the same as `where dyn A, dyn B: C`.
/// - Only additions have been made to trait methods, and only at the end of the method
///   list. Removing a method is a backwards incompatible change.
/// - All methods have the same ABI as previous versions. Method parameters and return
///   types must either match or share the same ABI.
///
/// [ref-obj-safety]: https://doc.rust-lang.org/reference/items/traits.html#object-safety
pub use dyntable_macro::dyntable;

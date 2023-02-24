use core::{
	alloc::Layout,
	ffi::c_void,
	marker::PhantomData,
	mem::{self, MaybeUninit},
	ptr::NonNull,
};

use std_alloc::{alloc::handle_alloc_error, boxed::Box};

use crate::{
	alloc::{AllocError, Allocator, Deallocator, GlobalAllocator, MemoryLayout},
	AsDyn,
	AssociatedDrop,
	AssociatedLayout,
	DynPtr,
	DynRef,
	DynRefMut,
	DynTrait,
	DynUnchecked,
	SubTable,
	VTableRepr,
};

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
	A: std_alloc::alloc::Allocator,
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

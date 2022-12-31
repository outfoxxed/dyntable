use std::{
	ffi::c_void,
	marker::PhantomData,
	ops::{Deref, DerefMut},
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
	/// Drop and deallocate a dyntable
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

/// FFI Safe `Box<dyn V>`
///
/// Owned version of [`Dyn`]
#[repr(C)]
pub struct DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	r#dyn: Dyn<V>,
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

impl<V> DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	/// Creates a new `DynBox<V>` from a type implementing `V`.
	pub fn new<'v, T: DynTable<'v, V::VTable>>(data: T) -> Self
	where
		V::VTable: 'v,
	{
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: Box::into_raw(Box::new(data)) as *mut c_void,
			},
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

impl<'v, T, V> From<Box<T>> for DynBox<V>
where
	T: DynTable<'v, V::VTable>,
	V: VTableRepr + ?Sized,
	V::VTable: 'v + DropTable,
{
	fn from(value: Box<T>) -> Self {
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: Box::into_raw(value) as *mut c_void,
			},
		}
	}
}

impl<V> Drop for DynBox<V>
where
	V: VTableRepr + ?Sized,
	V::VTable: DropTable,
{
	fn drop(&mut self) {
		unsafe {
			(*self.r#dyn.vtable).virtual_drop(self.dynptr);
		}
	}
}

pub use dyntable_macro::dyntable;

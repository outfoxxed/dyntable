use std::{
	ffi::c_void,
	marker::PhantomData,
	ops::{Deref, DerefMut},
};

/// Dyntable implementation details. You should not depend on these.
pub mod __private {
	use std::{ffi::c_void, marker::PhantomData, mem};

	use crate::{DropTable, Dyn, DynTable, VTable, VTableRepr};

	/// Trait that implies nothing, used for `VTable::Bounds`
	/// when no bounds are required
	pub trait NoBounds {}

	#[inline]
	pub fn dyn_vtable<V: VTableRepr + ?Sized>(r#dyn: &Dyn<V>) -> *const V::VTable {
		r#dyn.vtable
	}

	#[inline]
	pub fn dyn_ptr<V: VTableRepr + ?Sized>(r#dyn: &Dyn<V>) -> *mut c_void {
		r#dyn.dynptr
	}

	/// Struct used to evade the orphan rule, which prevents directly
	/// implementing DynTable for `T: DynTrait`
	pub struct DynImplTarget<T, V: VTable>(PhantomData<(T, V)>);

	/// Copy of DynTrait used to prevent a recursive impl
	pub unsafe trait DynTable2<'v, V: 'v + VTable> {
		const VTABLE: V;
		const STATIC_VTABLE: &'v V;
	}

	// would cause a recursive impl if `DynTable` was used instead of `DynTable2`
	unsafe impl<'v, T, V: 'v + VTable> DynTable<'v, V> for T
	where
		DynImplTarget<T, V>: DynTable2<'v, V>,
	{
		const STATIC_VTABLE: &'v V = DynImplTarget::<T, V>::STATIC_VTABLE;
		const VTABLE: V = DynImplTarget::<T, V>::VTABLE;
	}

	/// VTable wrapper that marks a VTable as Send.
	/// Usage of this type is unsafe.
	#[repr(transparent)]
	pub struct SendVTable<T: VTable>(T);
	/// VTable wrapper that marks a VTable as Sync
	/// Usage of this type is unsafe.
	#[repr(transparent)]
	pub struct SyncVTable<T: VTable>(T);
	/// VTable wrapper that marks a VTable as Send + Sync
	/// Usage of this type is unsafe.
	#[repr(transparent)]
	pub struct SendSyncVTable<T: VTable>(T);

	/// Usage of this type is unsafe.
	pub struct SendWrapper<T: ?Sized>(T);
	unsafe impl<T: ?Sized> Send for SendWrapper<T> {}

	/// Usage of this type is unsafe.
	pub struct SyncWrapper<T: ?Sized>(T);
	unsafe impl<T: ?Sized> Sync for SyncWrapper<T> {}

	/// Usage of this type is unsafe.
	pub struct SendSyncWrapper<T: ?Sized>(T);
	unsafe impl<T: ?Sized> Send for SendSyncWrapper<T> {}
	unsafe impl<T: ?Sized> Sync for SendSyncWrapper<T> {}

	unsafe impl<T: VTable> VTable for SendVTable<T> {
		type Bounds = SendWrapper<T::Bounds>;
	}

	unsafe impl<T: VTable> VTable for SyncVTable<T> {
		type Bounds = SyncWrapper<T::Bounds>;
	}

	unsafe impl<T: VTable> VTable for SendSyncVTable<T> {
		type Bounds = SendSyncWrapper<T::Bounds>;
	}

	unsafe impl<T: DropTable> DropTable for SendVTable<T> {
		unsafe fn virtual_drop(&self, instance: *mut c_void) {
			self.0.virtual_drop(instance);
		}
	}

	unsafe impl<T: DropTable> DropTable for SyncVTable<T> {
		unsafe fn virtual_drop(&self, instance: *mut c_void) {
			self.0.virtual_drop(instance);
		}
	}

	unsafe impl<T: DropTable> DropTable for SendSyncVTable<T> {
		unsafe fn virtual_drop(&self, instance: *mut c_void) {
			self.0.virtual_drop(instance);
		}
	}

	unsafe impl<'v, T: Send, V: 'v + VTable> DynTable<'v, SendVTable<V>> for T
	where
		DynImplTarget<T, V>: DynTable2<'v, V>,
	{
		// SAFETY: SendVTable is #[repr(transparent)]
		const STATIC_VTABLE: &'v SendVTable<V> =
			unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
		const VTABLE: SendVTable<V> = SendVTable(DynImplTarget::<T, V>::VTABLE);
	}

	unsafe impl<'v, T: Sync, V: 'v + VTable> DynTable<'v, SyncVTable<V>> for T
	where
		DynImplTarget<T, V>: DynTable2<'v, V>,
	{
		// SAFETY: SyncVTable is #[repr(transparent)]
		const STATIC_VTABLE: &'v SyncVTable<V> =
			unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
		const VTABLE: SyncVTable<V> = SyncVTable(DynImplTarget::<T, V>::VTABLE);
	}

	unsafe impl<'v, T: Send + Sync, V: 'v + VTable> DynTable<'v, SendSyncVTable<V>> for T
	where
		DynImplTarget<T, V>: DynTable2<'v, V>,
	{
		// SAFETY: SendSyncVTable is #[repr(transparent)]
		const STATIC_VTABLE: &'v SendSyncVTable<V> =
			unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
		const VTABLE: SendSyncVTable<V> = SendSyncVTable(DynImplTarget::<T, V>::VTABLE);
	}
}

/// Marker for dyntable traits
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

/// Trait used to retrieve an embedded VTable inside another VTable
pub trait SubTable<V: VTable>: VTable {
	fn subtable(&self) -> &V;
}

impl<V: VTable> SubTable<V> for V {
	fn subtable(&self) -> &V {
		&self
	}
}

/// Marker for representations of VTables to use in generics
pub trait VTableRepr {
	type VTable: VTable;
}

/// FFI safe wide pointer.
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

/// FFI Safe Box<dyn Trait>
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

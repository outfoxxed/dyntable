use core::{ffi::c_void, marker::PhantomData, mem};

use crate::{alloc::MemoryLayout, AssociatedDrop, AssociatedLayout, DynTrait, VTable};

/// Trait that implies nothing, used for `VTable::Bounds`
/// when no bounds are required
pub trait NoBounds {}

/// Struct used to evade the orphan rule, which prevents directly
/// implementing DynTrait for `T: DynTrait`
pub struct DynImplTarget<T, V: VTable>(PhantomData<(T, V)>);

/// Copy of DynTrait used to prevent a recursive impl
#[allow(clippy::missing_safety_doc)]
pub unsafe trait DynTraitProxy<'v, V: 'v + VTable> {
	const VTABLE: V;
	const STATIC_VTABLE: &'v V;
}

// would cause a recursive impl if `DynTrait` was used instead of `DynTraitProxy`
unsafe impl<'v, T, V: 'v + VTable> DynTrait<'v, V> for T
where
	DynImplTarget<T, V>: DynTraitProxy<'v, V>,
{
	const STATIC_VTABLE: &'v V = DynImplTarget::<T, V>::STATIC_VTABLE;
	const VTABLE: V = DynImplTarget::<T, V>::VTABLE;
}

// VTable trait wrappers

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

/// Wrapper type to mark an arbitrary `T` as Send.
///
/// Usage of this type is unsafe.
pub struct SendWrapper<T: ?Sized>(T);
unsafe impl<T: ?Sized> Send for SendWrapper<T> {}

/// Wrapper type to mark an arbitrary `T` as Sync.
///
/// Usage of this type is unsafe.
pub struct SyncWrapper<T: ?Sized>(T);
unsafe impl<T: ?Sized> Sync for SyncWrapper<T> {}

/// Wrapper type to mark an arbitrary `T` as Send + Sync.
///
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

unsafe impl<T: AssociatedDrop> AssociatedDrop for SendVTable<T> {
	#[inline(always)]
	unsafe fn virtual_drop(&self, instance: *mut c_void) {
		self.0.virtual_drop(instance);
	}
}

unsafe impl<T: AssociatedDrop> AssociatedDrop for SyncVTable<T> {
	#[inline(always)]
	unsafe fn virtual_drop(&self, instance: *mut c_void) {
		self.0.virtual_drop(instance);
	}
}

unsafe impl<T: AssociatedDrop> AssociatedDrop for SendSyncVTable<T> {
	#[inline(always)]
	unsafe fn virtual_drop(&self, instance: *mut c_void) {
		self.0.virtual_drop(instance);
	}
}

unsafe impl<T: AssociatedLayout> AssociatedLayout for SendVTable<T> {
	#[inline(always)]
	fn virtual_layout(&self) -> MemoryLayout {
		self.0.virtual_layout()
	}
}

unsafe impl<T: AssociatedLayout> AssociatedLayout for SyncVTable<T> {
	#[inline(always)]
	fn virtual_layout(&self) -> MemoryLayout {
		self.0.virtual_layout()
	}
}

unsafe impl<T: AssociatedLayout> AssociatedLayout for SendSyncVTable<T> {
	#[inline(always)]
	fn virtual_layout(&self) -> MemoryLayout {
		self.0.virtual_layout()
	}
}

unsafe impl<'v, T: Send, V: 'v + VTable> DynTrait<'v, SendVTable<V>> for T
where
	DynImplTarget<T, V>: DynTraitProxy<'v, V>,
{
	// SAFETY: SendVTable is #[repr(transparent)]
	const STATIC_VTABLE: &'v SendVTable<V> =
		unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
	const VTABLE: SendVTable<V> = SendVTable(DynImplTarget::<T, V>::VTABLE);
}

unsafe impl<'v, T: Sync, V: 'v + VTable> DynTrait<'v, SyncVTable<V>> for T
where
	DynImplTarget<T, V>: DynTraitProxy<'v, V>,
{
	// SAFETY: SyncVTable is #[repr(transparent)]
	const STATIC_VTABLE: &'v SyncVTable<V> =
		unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
	const VTABLE: SyncVTable<V> = SyncVTable(DynImplTarget::<T, V>::VTABLE);
}

unsafe impl<'v, T: Send + Sync, V: 'v + VTable> DynTrait<'v, SendSyncVTable<V>> for T
where
	DynImplTarget<T, V>: DynTraitProxy<'v, V>,
{
	// SAFETY: SendSyncVTable is #[repr(transparent)]
	const STATIC_VTABLE: &'v SendSyncVTable<V> =
		unsafe { mem::transmute(DynImplTarget::<T, V>::STATIC_VTABLE) };
	const VTABLE: SendSyncVTable<V> = SendSyncVTable(DynImplTarget::<T, V>::VTABLE);
}

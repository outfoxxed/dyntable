use std::{
	ffi::c_void,
	marker::PhantomData,
	ops::{Deref, DerefMut},
};

/// Marker for dyntable traits
pub unsafe trait DynTable<V: VTable> {
	/// The underlying VTable for the type this trait is applied to
	const VTABLE: V;
	const STATIC_VTABLE: &'static V;
}

/// Marker for dyntable traits that can be dropped
pub unsafe trait DynDropTable<D: DropTable>: DynTable<D::VTable> {
	/// The underlying DropTable for the type this trait is applied to
	const DROPTABLE: &'static D;
}

/// Marker trait for structs that are VTables
pub unsafe trait VTable: 'static {}

/// Trait used to drop objects behind a dyntable.
///
/// Only nessesary for the outermost nested vtable.
/// Embedded vtables do not need to, and probably should not
/// implement this trait.
pub unsafe trait DropTable: 'static {
	type VTable: VTable;

	/// Drop and deallocate a dyntable
	unsafe fn virtual_drop(&self, vtable: *const Self::VTable, instance: *mut c_void);
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

/// Marker for representations of DropTables to use in generics
pub trait DropTableRepr: VTableRepr {
	type DropTable: DropTable<VTable = Self::VTable>;
}

/// FFI safe wide pointer.
#[repr(C)]
pub struct Dyn<V: VTableRepr + ?Sized> {
	vtable: *const V::VTable,
	dynptr: *mut c_void,
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

/// FFI Safe Box<dyn Trait>
#[repr(C)]
pub struct DynBox<V: DropTableRepr + ?Sized> {
	r#dyn: Dyn<V>,
	droptable: *const V::DropTable,
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

impl<V: DropTableRepr + ?Sized> Deref for DynBox<V> {
	type Target = Dyn<V>;

	fn deref(&self) -> &Self::Target {
		&self.r#dyn
	}
}

impl<V: DropTableRepr + ?Sized> DerefMut for DynBox<V> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.r#dyn
	}
}

impl<V: DropTableRepr + ?Sized> DynBox<V> {
	pub fn new<T: DynDropTable<V::DropTable>>(data: T) -> Self {
		Self {
			r#dyn: Dyn {
				vtable: T::STATIC_VTABLE,
				dynptr: Box::into_raw(Box::new(data)) as *mut c_void,
			},
			droptable: T::DROPTABLE,
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

impl<V: DropTableRepr + ?Sized> Drop for DynBox<V> {
	fn drop(&mut self) {
		unsafe {
			(*self.droptable).virtual_drop(self.vtable, self.dynptr);
		}
	}
}

#[cfg(test)]
mod test {
	use std::{
		ffi::c_void,
		marker::PhantomData,
		ops::{Add, Sub},
	};

	use crate::{
		DropTable,
		DropTableRepr,
		Dyn,
		DynBox,
		DynDropTable,
		DynTable,
		SubTable,
		VTable,
		VTableRepr,
	};

	trait Incrementable<'lt, T: Add> {
		fn increment(&mut self, amount: &'lt T);
	}

	trait Decrementable<T: Sub> {
		fn decrement(&mut self, amount: T);
	}

	trait IncDec<'lt, T: Add + Sub>: Incrementable<'lt, T> + Decrementable<T> {}

	trait Get<'lt, T: Add + Sub>: IncDec<'lt, T> {
		fn get(&self) -> T;
	}

	struct IncrementableVTable<T: Add + 'static> {
		increment: fn(*mut c_void, *const T),
	}

	struct DecrementableVTable<T: Sub + 'static> {
		decrement: fn(*mut c_void, T),
	}

	struct IncDecVTable<T: Add + Sub + 'static> {
		increment_vtable: IncrementableVTable<T>,
		decrement_vtable: DecrementableVTable<T>,
	}

	struct GetVTable<T: Add + Sub + 'static> {
		incdec_vtable: IncDecVTable<T>,
		get: fn(*const c_void) -> T,
	}

	// Dropper for tests, will need individual tables in macro to account for ABIs
	struct FnDropTable<V: VTable> {
		drop: fn(*mut c_void),
		_vt: PhantomData<V>,
	}

	unsafe impl<V: VTable> DropTable for FnDropTable<V> {
		type VTable = V;

		unsafe fn virtual_drop(&self, _vtable: *const Self::VTable, instance: *mut c_void) {
			(self.drop)(instance);
		}
	}

	unsafe impl<T: Add> VTable for IncrementableVTable<T> {}
	unsafe impl<T: Sub> VTable for DecrementableVTable<T> {}
	unsafe impl<T: Add + Sub> VTable for IncDecVTable<T> {}
	unsafe impl<T: Add + Sub> VTable for GetVTable<T> {}

	impl<'lt, T: Add + Sub>
		SubTable<<(dyn Incrementable<'static, T> + 'static) as VTableRepr>::VTable> for IncDecVTable<T>
	{
		fn subtable(&self) -> &<(dyn Incrementable<'static, T> + 'static) as VTableRepr>::VTable {
			&self.increment_vtable
		}
	}

	impl<'lt, T: Add + Sub> SubTable<<(dyn Decrementable<T> + 'static) as VTableRepr>::VTable>
		for IncDecVTable<T>
	{
		fn subtable(&self) -> &<(dyn Decrementable<T> + 'static) as VTableRepr>::VTable {
			&self.decrement_vtable
		}
	}

	impl<'lt, T: Add + Sub> SubTable<<(dyn IncDec<'static, T> + 'static) as VTableRepr>::VTable>
		for GetVTable<T>
	{
		fn subtable(&self) -> &<(dyn IncDec<'static, T> + 'static) as VTableRepr>::VTable {
			&self.incdec_vtable
		}
	}

	impl<'lt, T: Add + Sub>
		SubTable<<(dyn Incrementable<'static, T> + 'static) as VTableRepr>::VTable> for GetVTable<T>
	{
		fn subtable(&self) -> &<(dyn Incrementable<'static, T> + 'static) as VTableRepr>::VTable {
			SubTable::<<dyn IncDec<'static, T> as VTableRepr>::VTable>::subtable(self).subtable()
		}
	}

	impl<'lt, T: Add + Sub> SubTable<<(dyn Decrementable<T> + 'static) as VTableRepr>::VTable>
		for GetVTable<T>
	{
		fn subtable(&self) -> &<(dyn Decrementable<T> + 'static) as VTableRepr>::VTable {
			SubTable::<<dyn IncDec<'static, T> as VTableRepr>::VTable>::subtable(self).subtable()
		}
	}

	unsafe impl<'lt, T: Add, D: Incrementable<'lt, T>> DynTable<IncrementableVTable<T>> for D {
		const STATIC_VTABLE: &'static IncrementableVTable<T> = &Self::VTABLE;
		const VTABLE: IncrementableVTable<T> = IncrementableVTable {
			increment: unsafe { std::mem::transmute(D::increment as fn(_, _)) },
		};
	}

	unsafe impl<T: Sub, D: Decrementable<T>> DynTable<DecrementableVTable<T>> for D {
		const STATIC_VTABLE: &'static DecrementableVTable<T> = &Self::VTABLE;
		const VTABLE: DecrementableVTable<T> = DecrementableVTable {
			decrement: unsafe { std::mem::transmute(D::decrement as fn(_, _)) },
		};
	}

	unsafe impl<'lt, T: Add + Sub, D: IncDec<'lt, T>> DynTable<IncDecVTable<T>> for D {
		const STATIC_VTABLE: &'static IncDecVTable<T> = &Self::VTABLE;
		const VTABLE: IncDecVTable<T> = IncDecVTable {
			increment_vtable: <D as DynTable<<dyn Incrementable<T> as VTableRepr>::VTable>>::VTABLE,
			decrement_vtable: <D as DynTable<<dyn Decrementable<T> as VTableRepr>::VTable>>::VTABLE,
		};
	}

	unsafe impl<'lt, T: Add + Sub, D: Get<'lt, T>> DynTable<GetVTable<T>> for D {
		const STATIC_VTABLE: &'static GetVTable<T> = &Self::VTABLE;
		const VTABLE: GetVTable<T> = GetVTable {
			incdec_vtable: <D as DynTable<<dyn IncDec<T> as VTableRepr>::VTable>>::VTABLE,
			get: unsafe { std::mem::transmute(D::get as fn(_) -> _) },
		};
	}

	unsafe impl<V: VTable, D: DynTable<V>> DynDropTable<FnDropTable<V>> for D {
		const DROPTABLE: &'static FnDropTable<V> = &FnDropTable {
			drop: |ptr| unsafe {
				std::ptr::drop_in_place(ptr as *mut D);
				std::alloc::dealloc(ptr as *mut u8, std::alloc::Layout::new::<D>());
			},
			_vt: PhantomData,
		};
	}

	impl<'lt, T: Add + 'static> VTableRepr for dyn Incrementable<'lt, T> {
		type VTable = IncrementableVTable<T>;
	}

	impl<T: Sub + 'static> VTableRepr for dyn Decrementable<T> {
		type VTable = DecrementableVTable<T>;
	}

	impl<'lt, T: Add + Sub + 'static> VTableRepr for dyn IncDec<'lt, T> {
		type VTable = IncDecVTable<T>;
	}

	impl<'lt, T: Add + Sub + 'static> VTableRepr for dyn Get<'lt, T> {
		type VTable = GetVTable<T>;
	}

	impl<'lt, T: Add + 'static> DropTableRepr for dyn Incrementable<'lt, T> {
		type DropTable = FnDropTable<IncrementableVTable<T>>;
	}

	impl<T: Sub + 'static> DropTableRepr for dyn Decrementable<T> {
		type DropTable = FnDropTable<DecrementableVTable<T>>;
	}

	impl<'lt, T: Add + Sub + 'static> DropTableRepr for dyn IncDec<'lt, T> {
		type DropTable = FnDropTable<IncDecVTable<T>>;
	}

	impl<'lt, T: Add + Sub + 'static> DropTableRepr for dyn Get<'lt, T> {
		type DropTable = FnDropTable<GetVTable<T>>;
	}

	impl<'lt, T: Add + 'static, V, R> Incrementable<'lt, T> for Dyn<R>
	where
		V: SubTable<<dyn Incrementable<'lt, T> as VTableRepr>::VTable>,
		R: VTableRepr<VTable = V> + ?Sized,
	{
		fn increment(&mut self, amount: &'lt T) {
			unsafe { ((*self.vtable).subtable().increment)(self.dynptr, amount) }
		}
	}

	impl<T: Sub + 'static, V, R> Decrementable<T> for Dyn<R>
	where
		V: SubTable<<dyn Decrementable<T> as VTableRepr>::VTable>,
		R: VTableRepr<VTable = V> + ?Sized,
	{
		fn decrement(&mut self, amount: T) {
			unsafe { ((*self.vtable).subtable().decrement)(self.dynptr, amount) }
		}
	}

	impl<'lt, T: Add + Sub + 'static, V, R> IncDec<'lt, T> for Dyn<R>
	where
		V: SubTable<<dyn IncDec<'lt, T> as VTableRepr>::VTable>
			+ SubTable<<dyn Incrementable<'lt, T> as VTableRepr>::VTable>
			+ SubTable<<dyn Decrementable<T> as VTableRepr>::VTable>,
		R: VTableRepr<VTable = V> + ?Sized,
	{
	}

	impl<'lt, T: Add + Sub + 'static, V, R> Get<'lt, T> for Dyn<R>
	where
		V: SubTable<<dyn Get<'lt, T> as VTableRepr>::VTable>
			+ SubTable<<dyn IncDec<'lt, T> as VTableRepr>::VTable>
			+ SubTable<<dyn Incrementable<'lt, T> as VTableRepr>::VTable>
			+ SubTable<<dyn Decrementable<T> as VTableRepr>::VTable>,
		R: VTableRepr<VTable = V> + ?Sized,
	{
		fn get(&self) -> T {
			unsafe {
				let vtable: &<dyn Get<'lt, T> as VTableRepr>::VTable = (&*self.vtable).subtable();
				(vtable.get)(self.dynptr)
			}
		}
	}

	#[test]
	fn test_dyn() {
		struct NumberHolder {
			num: i32,
		}

		impl Incrementable<'_, i32> for NumberHolder {
			fn increment(&mut self, amount: &i32) {
				self.num += amount;
			}
		}

		impl Decrementable<i32> for NumberHolder {
			fn decrement(&mut self, amount: i32) {
				self.num -= amount;
			}
		}

		impl IncDec<'_, i32> for NumberHolder {}
		impl Get<'_, i32> for NumberHolder {
			fn get(&self) -> i32 {
				self.num
			}
		}

		let mut dynbox: DynBox<dyn Get<'_, i32>> = DynBox::new(NumberHolder { num: 42 });

		println!("Num: {}", dynbox.get());

		dynbox.increment(&69);

		println!("Num: {}", dynbox.get());

		dynbox.decrement(22);

		println!("Num: {}", dynbox.get());
	}
}

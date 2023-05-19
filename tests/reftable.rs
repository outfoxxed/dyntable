use dyntable::{dyntable, DynBox, DynPtr, DynTrait, VTableRepr};

/// OriginalBase is the original base trait of MainTrait.
#[dyntable]
trait OriginalBase {
	extern "C" fn basefn(&self) -> i32;
}

/// UpdatedBase is a new ABI compatible update to OriginalBase.
#[dyntable]
trait UpdatedBase {
	extern "C" fn basefn(&self) -> i32;
	// New function not present in OriginalBase.
	extern "C" fn updatedfn(&self) -> i32;
}

#[dyntable]
trait MainTrait: OriginalBase
where
	&dyn OriginalBase:,
{
	extern "C" fn mainfn(&self) -> i32;
}

struct TargetStruct;

impl OriginalBase for TargetStruct {
	extern "C" fn basefn(&self) -> i32 {
		0
	}
}

impl MainTrait for TargetStruct {
	extern "C" fn mainfn(&self) -> i32 {
		1
	}
}

impl UpdatedBase for TargetStruct {
	extern "C" fn basefn(&self) -> i32 {
		2
	}

	extern "C" fn updatedfn(&self) -> i32 {
		3
	}
}

#[test]
fn reftable_updates() {
	let mut new_vt = <TargetStruct as DynTrait<<dyn MainTrait as VTableRepr>::VTable>>::VTABLE;

	let dynb = DynBox::<dyn MainTrait>::new(TargetStruct);
	let mut dynp = DynBox::into_raw(dynb);
	dynp.vtable = &new_vt;
	let dynb = unsafe { DynBox::from_raw(dynp) };
	assert_eq!(dynb.basefn(), 0);
	assert_eq!(dynb.mainfn(), 1);

	let updated_vt = <TargetStruct as DynTrait<<dyn UpdatedBase as VTableRepr>::VTable>>::VTABLE;
	new_vt.__vtable_OriginalBase =
		&updated_vt as *const _ as *const <dyn OriginalBase as VTableRepr>::VTable;
	let mut dynp = DynBox::into_raw(dynb);
	dynp.vtable = &new_vt;
	let dynb = unsafe { DynBox::from_raw(dynp) };
	assert_eq!(dynb.basefn(), 2);
	assert_eq!(dynb.mainfn(), 1);
}

// Ensure reftables can be inherited.

#[dyntable]
trait EnsureInheritance: MainTrait
where
	dyn MainTrait: OriginalBase,
{
}

#[dyntable]
trait EnsureInheritance2: EnsureInheritance
where
	dyn EnsureInheritance: MainTrait,
	dyn MainTrait: OriginalBase,
{
}

unsafe fn _ensure_inheritance(p: DynPtr<dyn EnsureInheritance2>) {
	let _ptr: *const <dyn OriginalBase as VTableRepr>::VTable = (*p.vtable)
		.__vtable_EnsureInheritance
		.__vtable_MainTrait
		.__vtable_OriginalBase;
}

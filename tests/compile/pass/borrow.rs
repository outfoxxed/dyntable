use dyntable::{dyntable, DynRef, DynRefMut, DynBox};

fn main() {}

#[dyntable(relax_abi = true)]
trait TestTrait {
	fn borrowed(&self);
	fn borrowed_mut(&mut self);
}

struct TestStruct {}

impl TestTrait for TestStruct {
	fn borrowed(&self) {}

	fn borrowed_mut(&mut self) {}
}

// call functions that borrow mutably and immutably without reborrowing

fn _call_borrowed_trait_fn(dynref: DynRef<dyn TestTrait>) {
	dynref.borrowed();
	dynref.borrowed();
}

fn _call_mut_borrowed_trait_fn(mut dynref: DynRefMut<dyn TestTrait>) {
	dynref.borrowed();
	dynref.borrowed_mut();
	dynref.borrowed();
	dynref.borrowed_mut();
}

// call functions taking mutable and immutable borrows

#[allow(unused)]
fn borrow_dyn(dynref: DynRef<dyn TestTrait>) {}
#[allow(unused)]
fn borrow_dyn_mut(dynref: DynRefMut<dyn TestTrait>) {}

fn _call_borrowing(dynref: DynRef<dyn TestTrait>) {
	borrow_dyn(dynref);
	borrow_dyn(dynref.borrow());
	borrow_dyn(dynref);
}

fn _call_mut_borrowing(mut dynref: DynRefMut<dyn TestTrait>) {
	borrow_dyn(dynref.borrow());
	borrow_dyn_mut(dynref.borrow_mut());
	borrow_dyn(dynref.borrow());
	borrow_dyn_mut(dynref.borrow_mut());
}

// borrowing must not impose additional lifetime restrictions

fn _call_and_return<'a>(dynref: DynRef<'a, dyn TestTrait>) -> DynRef<'a, dyn TestTrait> {
	dynref.borrow()
}

// Not possible to allow while upholding XOR mutability

/*
fn _call_and_return_mut<'a>(mut dynref: DynRefMut<'a, dyn TestTrait>) -> DynRefMut<'a, dyn TestTrait> {
	dynref.borrow_mut()
}

fn _call_and_return_shared_mut<'a>(dynref: DynRefMut<'a, dyn TestTrait>) -> DynRef<'a, dyn TestTrait> {
	dynref.borrow()
}
*/

fn _call_box_and_return<'a>(dynbox: &'a DynBox<dyn TestTrait>) -> DynRef<'a, dyn TestTrait> {
	dynbox.borrow()
}

fn _call_box_and_return_mut<'a>(dynbox: &'a mut DynBox<dyn TestTrait>) -> DynRefMut<'a, dyn TestTrait> {
	dynbox.borrow_mut()
}

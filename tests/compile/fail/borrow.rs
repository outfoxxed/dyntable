use dyntable::{dyntable, DynRefMut};

fn main() {}

#[dyntable]
trait TestTrait {}

fn _double_mut_borrow(mut dynref: DynRefMut<dyn TestTrait>) {
		let a = dynref.borrow_mut();
		let b = dynref.borrow_mut();
		let _ = (a, b);
}

fn _shared_mut_overlap1(mut dynref: DynRefMut<dyn TestTrait>) {
	let a = dynref.borrow();
	let b = dynref.borrow_mut();
	let _ = (a, b);
}

fn _shared_mut_overlap2(mut dynref: DynRefMut<dyn TestTrait>) {
	let a = dynref.borrow_mut();
	let b = dynref.borrow();
	let _ = (a, b);
}

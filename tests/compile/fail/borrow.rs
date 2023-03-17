use dyntable::{dyntable, DynRefMut};

fn main() {}

#[dyntable]
trait TestTrait {}

fn _double_mut_borrow(mut dynref: DynRefMut<dyn TestTrait>) {
	let a = DynRefMut::borrow_mut(&mut dynref);
	let b = DynRefMut::borrow_mut(&mut dynref);
	let _ = (a, b);
}

fn _shared_mut_overlap1(mut dynref: DynRefMut<dyn TestTrait>) {
	let a = DynRefMut::borrow(&dynref).clone();
	let b = DynRefMut::borrow_mut(&mut dynref);
	let _ = (a, b);
}

fn _shared_mut_overlap2(mut dynref: DynRefMut<dyn TestTrait>) {
	let a = DynRefMut::borrow_mut(&mut dynref);
	let b = DynRefMut::borrow(&dynref).clone();
	let _ = (a, b);
}

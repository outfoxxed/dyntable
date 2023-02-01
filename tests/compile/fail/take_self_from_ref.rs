use dyntable::{dyntable, DynRef, DynRefMut};

fn main() {}

#[dyntable(relax_abi = true)]
trait SelfTaker {
	fn takes_self(self);
}

fn _from_ref(dynref: DynRef<dyn SelfTaker>) {
	dynref.takes_self();
}

fn _from_mut_ref(dynref: DynRefMut<dyn SelfTaker>) {
	dynref.takes_self();
}

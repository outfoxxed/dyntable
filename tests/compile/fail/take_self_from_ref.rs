use dyntable::{dyntable, Dyn};

fn main() {}

#[dyntable(relax_abi = true)]
trait SelfTaker {
	fn takes_self(self);
}

fn _from_ref(dynref: &Dyn<dyn SelfTaker>) {
	dynref.takes_self();
}

fn _from_mut_ref(dynref: &mut Dyn<dyn SelfTaker>) {
	dynref.takes_self();
}

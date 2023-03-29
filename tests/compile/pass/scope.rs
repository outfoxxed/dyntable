use dyntable::{DynRef, AsDyn};
use test::PubTrait;

fn main() {}

mod test {
	use dyntable::dyntable;

	#[dyntable]
	pub trait PubTrait {
		extern "C" fn test(&self);
	}
}

fn test(test: DynRef<dyn PubTrait>) {
	// method is callable
	test.test();
	// vtable fields are accessable
	let vtable = unsafe { &*AsDyn::<dyn PubTrait>::dyn_vtable(&*test) };
	let _ = vtable.__drop;
	let _ = vtable.test;
	let _ = vtable.__generics;
}

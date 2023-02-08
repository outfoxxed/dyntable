use dyntable::{dyntable, DynRef};

fn main() {}

#[dyntable]
trait Trait1 {}

#[dyntable]
trait Trait2 {}

fn _upcast_unrelated(dynref: DynRef<dyn Trait1>) {
	let _: DynRef<dyn Trait2> = DynRef::upcast(dynref);
}

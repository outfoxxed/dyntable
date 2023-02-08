use dyntable::{dyntable, DynRef};

fn main() {}

#[dyntable]
trait Supertrait {}

#[dyntable]
trait Subtrait: Supertrait
where
	dyn Supertrait:,
{}

#[dyntable]
trait DoubleSubtrait: Subtrait
where
	dyn Subtrait: Supertrait,
{}

fn _upcast_direct(dynref: DynRef<dyn Subtrait>) {
	let _: DynRef<dyn Supertrait> = DynRef::upcast(dynref);
}

fn _upcast_indirect(dynref: DynRef<dyn DoubleSubtrait>) {
	let _: DynRef<dyn Supertrait> = DynRef::upcast(dynref);
}

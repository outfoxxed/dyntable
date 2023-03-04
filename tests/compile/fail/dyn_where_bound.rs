use dyntable::dyntable;

fn main() {}

trait ObjectSafe {}

#[dyntable]
trait Bounded
where
	// dyntable bound, not rust bound
	dyn ObjectSafe:,
{}

#[dyntable]
trait BoundedOnLifetime
where
	dyn 'foo + Foo:,
{}

#[dyntable]
trait BoundedByLifetime
where
	dyn Foo: 'foo,
{}

#[dyntable]
trait BoundedOnHRTB
where
	for<'a> dyn Foo:,
{}

#[dyntable]
trait BoundedByHRTB
where
	dyn Foo: for<'a> Bar,
{}

#[dyntable]
trait EmptyBound
where
	dyn :,
{}

#[dyntable]
trait BoundedOnMultiple
where
	dyn Foo + Bar:,
{}

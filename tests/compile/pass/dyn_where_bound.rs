use dyntable::dyntable;

fn main() {}

trait ObjectSafe {}

#[dyntable]
trait Bounded
where
	// bypasses dyntable trait resolution
	(dyn ObjectSafe):,
{}

use dyntable::dyntable;

trait ObjectSafe {}

#[dyntable]
trait Bounded
where
	// bypasses dyntable trait resolution
	(dyn ObjectSafe):,
{}

use dyntable::dyntable;

trait ObjectSafe {}

#[dyntable]
trait Bounded
where
	// dyntable bound, not rust bound
	dyn ObjectSafe:,
{}

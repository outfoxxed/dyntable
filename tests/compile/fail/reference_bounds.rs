use dyntable::dyntable;

fn main() {}

// Reference qualifiers can only be used on toplevel subtables.

#[dyntable]
trait BaseTrait {}

#[dyntable]
trait InheritingTrait: BaseTrait
where
	dyn BaseTrait:,
{
}

#[dyntable]
trait DoubleInheritingTrait: InheritingTrait
where
	dyn InheritingTrait: BaseTrait,
	// error
	&dyn BaseTrait:,
{
}

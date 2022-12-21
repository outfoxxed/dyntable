//! This test ensures specifying multiple paths to an inherited
//! trait bound is an error.

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait BaseTrait {}

#[dyntable]
trait Level1Trait1: BaseTrait
where
	dyn BaseTrait:,
{}

#[dyntable]
trait Level1Trait2: BaseTrait
where
	dyn BaseTrait:,
{}

#[dyntable]
trait Level2Trait: Level1Trait1 + Level1Trait2
where
	dyn Level1Trait1: BaseTrait,
	// this is an error because the path to BaseTrait has already
	// been specified above
	dyn Level1Trait2: BaseTrait,
{}

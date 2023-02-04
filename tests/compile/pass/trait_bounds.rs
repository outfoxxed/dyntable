//! This test ensures well formed #[dyntable] trait bounds
//! compile successfully. It also ensures that methods defined
//! in bound traits are callable.

use dyntable::{Dyn, dyntable};

fn main() {}

// Methods should be inherited through bounds.

#[dyntable]
trait BaseTrait {
	extern "C" fn base_method(&self);
}

#[dyntable]
trait Level1Trait: BaseTrait
where
	dyn BaseTrait:,
{}

fn call_base_from_level1(instance: &Dyn<dyn Level1Trait>) {
	instance.base_method();
}

#[dyntable]
trait Level2Trait: Level1Trait
where
	dyn Level1Trait: BaseTrait,
{}

fn call_base_from_level2(instance: &Dyn<dyn Level2Trait>) {
	instance.base_method();
}

// Multiple traits with bounds on a supertrait
// merging back together in a trait that inherits both

#[dyntable]
trait Level3Trait1: Level2Trait
where
	dyn Level2Trait: Level1Trait,
	dyn Level1Trait: BaseTrait,
{}

#[dyntable]
trait Level3Trait2: Level2Trait
where
	dyn Level2Trait: Level1Trait,
	dyn Level1Trait: BaseTrait,
{}

// Both paths through the bound inheritance tree should work

#[dyntable]
trait Level4Trait1: Level3Trait1 + Level3Trait2
where
	dyn Level3Trait1: Level2Trait,
	dyn Level3Trait2:, // path already specified
	dyn Level2Trait: Level1Trait,
	dyn Level1Trait: BaseTrait,
{}

fn call_base_from_level4_1(instance: &Dyn<dyn Level4Trait1>) {
	instance.base_method();
}

#[dyntable]
trait Level4Trait2: Level3Trait1 + Level3Trait2
where
	dyn Level3Trait1:, // path specified below
	dyn Level3Trait2: Level2Trait,
	dyn Level2Trait: Level1Trait,
	dyn Level1Trait: BaseTrait,
{}

fn call_base_from_level4_2(instance: &Dyn<dyn Level4Trait2>) {
	instance.base_method();
}

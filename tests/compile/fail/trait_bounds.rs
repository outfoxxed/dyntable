//! This test ensures that a #[dyntable] trait cannot have implicit
//! bound inheritance (will hide implemented methods from the user),
//! or have bounds on non #[dyntable] traits.

// FIXME: extremely confusing or misleading error messages

use dyntable::dyntable;

fn main() {}

trait NonDyntableTrait {}

/// Inherit a non dyn trait without a dyn entry in the where clause
#[dyntable]
trait NonDynBound: NonDyntableTrait {}

/// Inherit a non dyn trait with a dyn entry in the where clause
#[dyntable]
trait NonDynBoundWithClause: NonDyntableTrait
where
	dyn NonDyntableTrait:,
{}

// --

#[dyntable]
trait DyntableTrait {}

/// Inherit a dyn trait without adding a dyn entry in the where clause
#[dyntable]
trait DynBoundWithoutClause: DyntableTrait {}

// --

#[dyntable]
trait RequiredTrait {}

#[dyntable]
trait ImplementedTrait: RequiredTrait
where
	dyn RequiredTrait:,
{}

/// Inherit a dyn trait without explicitly specifying its bounds in
/// its dyn entry in the where clause
#[dyntable]
trait MissingExplicitBoundInheritance: ImplementedTrait
where
	dyn ImplementedTrait:,
{}

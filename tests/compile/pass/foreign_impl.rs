//! Ensure dyntable traits can be implemented on foreign types.

use dyntable::*;

fn main() {}

#[dyntable]
trait Trait {}

#[dyntable]
trait BoundedTrait: Trait
where
	dyn Trait:,
{
}

impl Trait for () {}
impl BoundedTrait for () {}

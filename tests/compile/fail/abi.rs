//! This test ensures that implicit abi mismatches are not allowed
//! in #[dyntable] traits

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait ImplicitRustAbiImplicitUnrelax {
	fn test(&self);
}

#[dyntable(relax_abi = false)]
trait ImplicitRustAbiExplicitUnrelax {
	fn test(&self);
}

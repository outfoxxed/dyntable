//! This test ensures that implicit abi mismatches are not allowed
//! in #[dyntable] traits

use dyntable::dyntable;

fn main() {}

#[dyntable(abi = C)]
trait ImplicitRustAbi {
	fn test(&self);
}

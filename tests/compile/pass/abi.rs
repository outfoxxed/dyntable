//! This test ensures function and struct definitions in
//! #[dyntable] traits follow the abi and repr provided to
//! the #[dyntable] macro

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait DefaultCAbi {
	extern "C" fn test(&self);
}

#[dyntable(abi = C)]
trait ExplicitCAbi {
	extern "C" fn test(&self);
}

#[dyntable(abi = Rust)]
trait ExplicitRustAbi {
	fn implicit_abi(&self);
	extern "Rust" fn explicit_abi(&self);
}

#[dyntable(repr = Rust)]
trait ExplicitRustRepr {
	extern "C" fn test(&self);
}

#[dyntable(repr = C)]
trait ExplictCRepr {
	extern "C" fn test(&self);
}

#[dyntable(abi = C, repr = C)]
trait ExplicitCAbiRepr {
	extern "C" fn test(&self);
}

#[dyntable(abi = Rust, repr = Rust)]
trait ExplicitRustAbiRepr {
	extern "Rust" fn test(&self);
}

#[dyntable]
trait MismatchedRustAbi {
	extern "Rust" fn test(&self);
}

#[dyntable(abi = Rust)]
trait MismatchedCAbi {
	extern "C" fn test(&self);
}

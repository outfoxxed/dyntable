//! This test ensures function and struct definitions in
//! #[dyntable] traits follow the abi and repr provided to
//! the #[dyntable] macro

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait DefaultExplicit {
	extern "C" fn test(&self);
}

#[dyntable(relax_abi = false)]
trait ExplicitUnrelaxed {
	extern "C" fn test(&self);
}

#[dyntable(relax_abi = true)]
trait ExplicitRelaxed {
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

#[dyntable(relax_abi = false, repr = C)]
trait ExplicitUnrelaxedCRepr {
	extern "C" fn test(&self);
}

#[dyntable(relax_abi = true, repr = Rust)]
trait ExplicitRelaxedRustRepr {
	fn implicit_abi(&self);
	extern "Rust" fn explicit_abi(&self);
}

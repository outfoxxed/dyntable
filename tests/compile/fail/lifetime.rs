//! This test ensures that lifetime generics in #[dyntable] traits
//! are sound

use dyntable::dyntable;
use core::marker::PhantomData;

fn main() {}

struct LifetimeStruct<'a>(PhantomData<&'a ()>);

#[dyntable(relax_abi = true)]
trait UnboundedGeneric<'a, A> {
	// `A` should be bounded by `'a`
	fn foo(&self) -> &'a A;
}

#[dyntable(relax_abi = true)]
trait BoundedLifetime {
	// bounded fn generic lifetimes are disallowed
	fn foo<'a, 'b: 'a>(&'b self, a: &'a A) -> &'a A;
}

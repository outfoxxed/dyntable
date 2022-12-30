//! This test ensures that lifetime generics in #[dyntable] traits
//! successfully compile

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait BaseLTTrait<'a, A> {
	extern "C" fn base_longref(&self) -> &'a A;
}

#[dyntable]
trait ChildLTTrait<'a, 'b, A, B>: BaseLTTrait<'a, A>
where
	dyn BaseLTTrait<'a, A>:,
{
	extern "C" fn child_longref(&self) -> &'b B;
}

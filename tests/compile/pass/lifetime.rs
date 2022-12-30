//! This test ensures that lifetime generics in #[dyntable] traits
//! successfully compile

use dyntable::dyntable;

fn main() {}

#[dyntable]
trait SelfLTTrait<T> {
	extern "C" fn get_with_self_lt<'s>(&'s self) -> &'s T;
}

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

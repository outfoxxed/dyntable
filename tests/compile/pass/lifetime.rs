//! This test ensures that lifetime generics in #[dyntable] traits
//! successfully compile

use dyntable::dyntable;
use core::marker::PhantomData;

fn main() {}

struct LifetimeStruct<'a>(PhantomData<&'a ()>);

#[dyntable(relax_abi = true)]
trait SelfLTTrait<T> {
	fn get_with_self_lt<'s>(&'s self) -> &'s T;
	fn lt_struct<'s>(&'s self) -> LifetimeStruct<'s>;
	fn lt_struct_implicit(&self) -> LifetimeStruct;
}

#[dyntable(relax_abi = true)]
trait PassthroughLTTrait<'s, T> {
	fn pass_t<'a>(&self, x: &'a T) -> &'a T;
	fn pass_lt_struct<'a>(&self, x: LifetimeStruct<'a>) -> LifetimeStruct<'a>;
}

#[dyntable(relax_abi = true)]
trait BaseLTTrait<'a, A: 'a> {
	fn base_longref(&self) -> &'a A;
	fn take_longref(self) -> &'a A;
	fn lt_struct(&self) -> LifetimeStruct<'a>;
	fn lt_struct_by_value(self) -> LifetimeStruct<'a>;
}

#[dyntable(relax_abi = true)]
trait ChildLTTrait<'a, 'b, A: 'a, B: 'b>: BaseLTTrait<'a, A>
where
	dyn BaseLTTrait<'a, A>:,
{
	fn child_longref(&self) -> &'b B;
}

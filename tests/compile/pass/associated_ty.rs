use dyntable::*;

fn main() {}

#[dyntable]
trait BaseTrait {}

#[dyntable]
trait Supertrait: BaseTrait
where
	dyn BaseTrait:,
{}

#[dyntable(relax_abi = true)]
trait WithBounds<A: Clone>: Supertrait
where
	dyn Supertrait: BaseTrait,
{
	type A: Clone;

	fn flip_generic_a<'a>(&self, a: &'a A) -> &'a A;
	fn flip_associated_a<'a>(&self, a: &'a Self::A) -> &'a Self::A;
	fn move_generic_a(self, a: A) -> A;
	fn move_associated_a(self, a: Self::A) -> Self::A;
}

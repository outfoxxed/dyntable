use dyntable::*;

fn main() {}

struct TImpl;
struct VImpl;

#[dyntable]
trait Single<T = TImpl> {}

#[dyntable(relax_abi = true)]
trait WithNonDefault<T, V = VImpl> {
	fn get_t(&self) -> T;
	fn get_v(&self) -> V;
}

#[dyntable]
trait WithAssociatedType<T = TImpl> {
	type Associated;
}

struct SingleTest(DynBox<dyn Single>);
struct WithNonDefaultTest(DynBox<dyn WithNonDefault<TImpl>>);
struct WithAssociatedTypeTest(DynBox<dyn WithAssociatedType<Associated = ()>>);

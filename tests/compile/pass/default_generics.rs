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

struct SingleTest(DynBox<dyn Single>);
struct WithNonDefaultTest(DynBox<dyn WithNonDefault<TImpl>>);

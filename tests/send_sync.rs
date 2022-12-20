use std::marker::PhantomData;

use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TSend<T>: Send {
	extern "C" fn send_test(&self) -> &T;
}

struct SSend<T>(PhantomData<T>);

impl<T: Send> TSend<T> for SSend<T> {
	extern "C" fn send_test(&self) -> &T {
		panic!("not implemented")
	}
}

#[test]
fn test() {
	let dynbox = DynBox::<dyn TSend<i32>>::new(SSend(PhantomData));

	fn require_send<T: Send>(_: &T) {}

	require_send(&dynbox);
}

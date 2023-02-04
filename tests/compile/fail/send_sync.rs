use dyntable::{DynBox, dyntable};

#[dyntable]
trait TestTrait {}

fn require_send<T: Send>(_: T) {}
fn require_sync<T: Sync>(_: T) {}

struct Dummy;
impl TestTrait for Dummy {}

fn main() {
	let dynbox = DynBox::<dyn TestTrait>::new(Dummy);
	let dynbox2 = DynBox::<dyn TestTrait>::new(Dummy);

	// &Dyn<dyn TestTrait> should not be Send
	require_send(&*dynbox);
	// &Dyn<dyn TestTrait> should not be Sync
	require_sync(&*dynbox);

	// &Dyn<dyn TestTrait> should not be Send
	require_send(dynbox);
	// &Dyn<dyn TestTrait> should not be Sync
	require_sync(dynbox2);
}

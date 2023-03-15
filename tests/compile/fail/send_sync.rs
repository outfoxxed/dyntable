use dyntable::{DynBox, dyntable};

fn main() {}

#[dyntable]
trait TestTrait {}

#[dyntable]
trait TestSend: Send {}

fn require_send<T: Send>(_: T) {}
fn require_sync<T: Sync>(_: T) {}

struct Dummy;
impl TestTrait for Dummy {}
impl TestSend for Dummy {}

fn not_present() {
	let mut dynbox = DynBox::<dyn TestTrait>::new(Dummy);
	let dynbox2 = DynBox::<dyn TestTrait>::new(Dummy);

	// DynMutRef<dyn TestTrait> should not be Send
	require_send(DynBox::borrow_mut(&mut dynbox));
	// DynMutRef<dyn TestTrait> should not be Sync
	require_sync(DynBox::borrow_mut(&mut dynbox));

	// DynRef<dyn TestTrait> should not be Send
	require_send(DynBox::borrow(&dynbox));
	// DynRef<dyn TestTrait> should not be Sync
	require_sync(DynBox::borrow(&dynbox));

	// DynBox<dyn TestTrait> should not be Send
	require_send(dynbox);
	// DynBox<dyn TestTrait> should not be Sync
	require_sync(dynbox2);
}

fn send_not_sync() {
	let mut dynbox = DynBox::<dyn TestSend>::new(Dummy);

	// DynRef(Mut)<dyn TestSend> should not be Send/Sync, as TestSend is not Sync
	require_send(DynBox::borrow_mut(&mut dynbox));
	require_send(DynBox::borrow(&dynbox));
	require_sync(DynBox::borrow_mut(&mut dynbox));
	require_sync(DynBox::borrow(&dynbox));

	// DynBox<dyn TestSend> should not be Sync
	require_sync(dynbox);
}

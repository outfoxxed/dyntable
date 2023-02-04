use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TSend: Send {}

#[dyntable]
trait TSync: Sync {}

#[dyntable]
trait TSendSync: Send + Sync {}

struct SSend;
struct SSync;
struct SSendSync;

impl TSend for SSend {}
impl TSync for SSync {}
impl TSendSync for SSendSync {}

#[dyntable]
trait UnboundedTrait {}

struct Unbounded;
impl UnboundedTrait for Unbounded {}

fn require_send<T: Send>(_: T) {}
fn require_sync<T: Sync>(_: T) {}
fn require_send_sync<T: Send + Sync>(_: T) {}

#[test]
fn test_bounded() {
	let send = DynBox::<dyn TSend>::new(SSend);
	let sync = DynBox::<dyn TSync>::new(SSync);
	let send_sync = DynBox::<dyn TSendSync>::new(SSendSync);

	require_send(&sync);
	require_sync(&sync);
	require_send_sync(&send_sync);

	require_send(send);
	require_sync(sync);
	require_send_sync(send_sync);
}

#[test]
fn test_added() {
	let send = DynBox::<dyn UnboundedTrait + Send>::new(Unbounded);
	let sync = DynBox::<dyn UnboundedTrait + Sync>::new(Unbounded);
	let send_sync = DynBox::<dyn UnboundedTrait + Send + Sync>::new(Unbounded);

	require_send(&*sync);
	require_sync(&*sync);
	require_send_sync(&*send_sync);

	require_send(send);
	require_sync(sync);
	require_send_sync(send_sync);
}

use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait BaseTrait {
	extern "C" fn base_fn(&self);
}

#[dyntable]
trait Edge1: BaseTrait
where
	dyn BaseTrait:,
{
	extern "C" fn edge1(&self);
	extern "C" fn name_conflict(&self);
}

#[dyntable]
trait Edge2<T>: BaseTrait
where
	dyn BaseTrait:,
{
	extern "C" fn edge2(&self, t: T);
	extern "C" fn name_conflict(&self);
}

#[dyntable]
trait Edge2_1<T>: Edge2<T>
where
	dyn Edge2<T>: BaseTrait,
{
	extern "C" fn edge2_1(&self);
}

#[dyntable]
trait Diamond<T>: Edge1 + Edge2_1<T>
where
	dyn Edge1: BaseTrait,
	dyn Edge2_1<T>: Edge2<T>,
{
	extern "C" fn diamond(&self);
}

struct TestStruct<T>(T);

impl<T> BaseTrait for TestStruct<T> {
	extern "C" fn base_fn(&self) {}
}

impl<T> Edge1 for TestStruct<T> {
	extern "C" fn edge1(&self) {}

	extern "C" fn name_conflict(&self) {}
}

impl<T> Edge2<T> for TestStruct<T> {
	extern "C" fn edge2(&self, _t: T) {}

	extern "C" fn name_conflict(&self) {}
}

impl<T> Edge2_1<T> for TestStruct<T> {
	extern "C" fn edge2_1(&self) {}
}

impl<T> Diamond<T> for TestStruct<T> {
	extern "C" fn diamond(&self) {}
}

#[test]
fn test() {
	let dynbox = DynBox::<dyn Diamond<i32>>::new(TestStruct(0));

	dynbox.base_fn();
	dynbox.edge1();
	dynbox.edge2(0);
	Edge1::name_conflict(&*dynbox);
	Edge2::name_conflict(&*dynbox);
	dynbox.edge2_1();
	dynbox.diamond();
}

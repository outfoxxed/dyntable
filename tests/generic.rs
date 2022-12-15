use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TestTable<T: Clone> {
	extern "C" fn get(&self) -> T;
}

struct TestStruct<T> {
	value: T,
}

impl<T: Clone> TestTable<T> for TestStruct<T> {
	extern "C" fn get(&self) -> T {
		self.value.clone()
	}
}

#[test]
fn test() {
	let dynbox = DynBox::<dyn TestTable<i32>>::new(TestStruct { value: 42 });
	assert_eq!(dynbox.get(), 42);
}

#[test]
fn test_reference() {
	#[derive(Debug, PartialEq)]
	struct Test(i32);

	let test = Test(42);
	let tref = &test;
	let tstr = TestStruct { value: tref };

	let tref2: &Test = {
		let dynbox = DynBox::<dyn TestTable<&Test>>::new(tstr);
		dynbox.get()
	};
	assert_eq!(tref, tref2);
}

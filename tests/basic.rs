use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TestTable {
	extern "C" fn get(&self) -> i32;
	extern "C" fn set(&mut self, value: i32);
}

#[dyntable]
unsafe trait UnsafeTrait {}

struct TestStruct {
	number: i32,
}

impl TestTable for TestStruct {
	extern "C" fn get(&self) -> i32 {
		self.number
	}

	extern "C" fn set(&mut self, value: i32) {
		self.number = value;
	}
}

#[test]
fn test() {
	let mut dynbox = DynBox::<dyn TestTable>::new(TestStruct { number: 42 });
	assert_eq!(dynbox.get(), 42);
	let v = dynbox.get();
	dynbox.set(v + 10);
	assert_eq!(dynbox.get(), 52);
}

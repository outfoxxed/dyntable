use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TestTable {
	extern "C" fn get(&self) -> i32;
	extern "C" fn set(&mut self, value: i32);
	extern "C" fn take(self) -> i32;
	extern "C" fn take_add(self, value: i32) -> i32;
}

#[dyntable]
unsafe trait UnsafeTrait {}

#[repr(C)]
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

	extern "C" fn take(self) -> i32 {
		self.number
	}

	extern "C" fn take_add(self, value: i32) -> i32 {
		self.number + value
	}
}

#[test]
fn test() {
	let mut dynbox = DynBox::<dyn TestTable>::new(TestStruct { number: 42 });

	assert_eq!(dynbox.get(), 42);
	let v = dynbox.get();
	dynbox.set(v + 10);
	assert_eq!(dynbox.get(), 52);
	assert_eq!(dynbox.take(), 52);

	let dynbox = DynBox::<dyn TestTable>::new(TestStruct { number: 42 });
	assert_eq!(dynbox.take_add(10), 52);
}

use dyntable::DynBox;
use dyntable_macro::dyntable;

#[dyntable]
trait TestTable<T: Clone> {
	extern "C" fn get(&self) -> T;
	extern "C" fn get_ref(&self) -> &T;
	extern "C" fn get_mut_ref(&mut self) -> &mut T;
	extern "C" fn set(&mut self, value: T);
}

struct TestStruct<T> {
	value: T,
}

impl<T: Clone> TestTable<T> for TestStruct<T> {
	extern "C" fn get(&self) -> T {
		self.value.clone()
	}

	extern "C" fn get_ref(&self) -> &T {
		&self.value
	}

	extern "C" fn get_mut_ref(&mut self) -> &mut T {
		&mut self.value
	}

	extern "C" fn set(&mut self, value: T) {
		self.value = value;
	}
}

#[test]
fn basic() {
	let mut dynbox = DynBox::<dyn TestTable<i32>>::new(TestStruct { value: 42 });
	assert_eq!(dynbox.get_ref(), &42);
	assert_eq!(dynbox.get(), 42);
	dynbox.set(45);
	assert_eq!(dynbox.get(), 45);
}

#[test]
fn reference_in_type() {
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

#[test]
fn reference_to_type() {
	#[derive(Clone, Debug, PartialEq)]
	struct Test(i32);

	let mut dynbox = DynBox::<dyn TestTable<Test>>::new(TestStruct { value: Test(42) });

	let r: &Test = dynbox.get_ref();
	assert_eq!(r.0, 42);

	let r: &mut Test = dynbox.get_mut_ref();
	r.0 += 42;
	assert_eq!(dynbox.get_ref().0, 84);
}

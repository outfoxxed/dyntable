use std::{
	env,
	fs,
	process::{Command, Stdio},
};

use dyntable::*;

#[test]
fn ffi() {
	fs::create_dir_all("target/ffitest").unwrap();

	Command::new(env::var("CC").expect("Missing CC environment var"))
		.args([
			"-Werror",
			"-shared",
			"-o",
			"target/ffitest/libffi.so",
			"tests/ffi.c",
		])
		.stdout(Stdio::inherit())
		.stderr(Stdio::inherit())
		.status()
		.unwrap();

	unsafe {
		let lib = libloading::Library::new("./target/ffitest/libffi.so").unwrap();

		let c_increment_bounded = lib
			.get::<unsafe extern "C" fn(DynRefMut<dyn BoundedTrait>)>(b"increment_bounded")
			.unwrap();
		let c_get_parent = lib
			.get::<unsafe extern "C" fn(DynRef<dyn ParentTrait>) -> i32>(b"get_parent")
			.unwrap();

		let mut rust_value = DynBox::<dyn BoundedTrait>::new(RustValue { value: 0 });

		c_increment_bounded(DynBox::borrow_mut(&mut rust_value));
		assert_eq!(rust_value.get(), 1);
		assert_eq!(
			rust_value.get(),
			c_get_parent(DynRef::upcast(DynBox::borrow(&rust_value)))
		);
	}
}

#[dyntable]
trait ParentTrait {
	extern "C" fn get(&self) -> i32;
}

#[dyntable]
trait BoundedTrait: ParentTrait
where
	dyn ParentTrait:,
{
	extern "C" fn set(&mut self, value: i32);
}

// no #[repr(C)]
struct RustValue {
	value: i32,
}

impl ParentTrait for RustValue {
	extern "C" fn get(&self) -> i32 {
		self.value
	}
}

impl BoundedTrait for RustValue {
	extern "C" fn set(&mut self, value: i32) {
		self.value = value;
	}
}

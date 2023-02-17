use std::{
	env,
	ffi::c_void,
	fs,
	process::{Command, Stdio},
};

use dyntable::{alloc::Deallocator, *};

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

		let debug_flags = lib
			.get::<unsafe extern "C" fn() -> *mut DebugFlags>(b"get_debug_flags")
			.unwrap()();
		let new_c_value = lib
			.get::<unsafe extern "C" fn() -> DynPtr<dyn BoundedTrait>>(b"new_c_value")
			.unwrap();
		let dealloc_c_value = lib
			.get::<unsafe extern "C" fn(*mut c_void)>(b"dealloc_c_value")
			.unwrap();
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

		#[derive(Copy, Clone)]
		struct CDeallocator {
			dealloc: unsafe extern "C" fn(*mut c_void),
		}

		let c_dealloc = CDeallocator {
			dealloc: *dealloc_c_value,
		};

		impl Deallocator for CDeallocator {
			unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, _: alloc::MemoryLayout) {
				(self.dealloc)(ptr.as_ptr() as *mut c_void)
			}
		}

		assert_eq!((*debug_flags).cdrop_calls, 0);
		assert_eq!((*debug_flags).cdealloc_calls, 0);

		let mut c_value = DynBox::from_raw_in(new_c_value(), c_dealloc);
		assert_eq!(c_value.get(), 0);
		c_value.set(c_value.get() + 1);
		assert_eq!(c_value.get(), 1);
		drop(c_value);

		assert_eq!((*debug_flags).cdrop_calls, 1);
		assert_eq!((*debug_flags).cdealloc_calls, 1);
	}
}

#[repr(C)]
struct DebugFlags {
	cdealloc_calls: u32,
	cdrop_calls: u32,
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

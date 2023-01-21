use dyntable::dyntable;

fn main() {}

#[repr(C)]
struct Target(i32);

// mutable and immutable borrows of self are supported

#[dyntable]
trait BorrowSelf {
	extern "C" fn immutable(&self);
	extern "C" fn mutable(&mut self);
}

impl BorrowSelf for Target {
	extern "C" fn immutable(&self) {}

	extern "C" fn mutable(&mut self) {}
}

// taking self by value is supported

#[dyntable]
trait ByValue {
	extern "C" fn by_value(self);
}

impl ByValue for Target {
	extern "C" fn by_value(self) {}
}

// arguments of type generics are supported

#[dyntable]
trait GenericArg<T> {
	extern "C" fn test(&self, arg: T);
}

// _ is a supported argument name

#[dyntable]
trait UnderscoreArg<T> {
	extern "C" fn borrow(&self, _: T);
	// taking by value creates an intermediary function, which also
	// cannot have a parameter named _
	extern "C" fn by_value(self, _: T);
}

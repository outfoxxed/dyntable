use dyntable::dyntable;

fn main() {}

// associated functions are not supported

#[dyntable]
trait Associated {
	fn associated();
}

// self must be the first parameter

#[dyntable]
trait WrongFirstArg {
	fn test(foo: Bar);
}

// self must be implicit (not typed)

#[dyntable]
trait TypedSelf {
	fn test(self: Self);
}

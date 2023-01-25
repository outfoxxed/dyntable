use test::{PrivTrait, PrivTraitVTable};

fn main() {}

mod test {
	use dyntable::dyntable;

	#[dyntable]
	trait PrivTrait {
		extern "C" fn test(&self);
	}
}

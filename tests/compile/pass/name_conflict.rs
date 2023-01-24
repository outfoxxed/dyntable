use dyntable::dyntable;

fn main() {}

// Check for name conflicts with implementation fn names, excluding
// double underscore names.
//
// Since rust functions may not be overloaded the parameters
// do not matter.

#[dyntable(relax_abi = true)]
trait NameConflict {
	fn dyn_vtable(&self);
	fn dyn_ptr(&self);
	fn dyn_dealloc(&self);
	fn subtable(&self);
	fn virtual_drop(&self);
}

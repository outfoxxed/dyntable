mod def;
mod imp;

pub use def::{fix_vtable_associated_types, gen_vtable, visit_type_paths};
pub use imp::gen_impl;

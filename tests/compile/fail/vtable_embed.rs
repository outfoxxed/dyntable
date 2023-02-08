//! Ensure VTables without required metadata don't work where nessesary.

use dyntable::{dyntable, DynBox};

fn main() {}

#[dyntable(drop = none)]
trait NoDrop {}

#[dyntable(embed_layout = false)]
trait NoLayout {}

#[dyntable(drop = none, embed_layout = false)]
trait NoMeta {}

// ensure missing meta dyntraits can't be used in a box

struct NoDropBox(DynBox<dyn NoDrop>);
struct NoLayoutBox(DynBox<dyn NoLayout>);
struct NoMetaBox(DynBox<dyn NoMeta>);

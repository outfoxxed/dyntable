//! Ensure VTables still work with and without embedded metadata.

use dyntable::{dyntable, DynPtr, DynBox};

fn main() {}

#[dyntable]
trait Default {}

#[dyntable(drop = none)]
trait NoDrop {}

#[dyntable(embed_layout = false)]
trait NoLayout {}

#[dyntable(drop = none, embed_layout = false)]
trait NoMeta {}

// ensure all can be used in a ptr or ref

struct DefaultPtr(DynPtr<dyn Default>);
struct NoDropPtr(DynPtr<dyn NoDrop>);
struct NoLayoutPtr(DynPtr<dyn NoLayout>);
struct NoMetaPtr(DynPtr<dyn NoMeta>);

// ensure full meta dyntraits can be used in a box

struct DefaultBox(DynBox<dyn Default>);

use syn::{
	parse::Parse,
	punctuated::Punctuated,
	token,
	Attribute,
	Generics,
	Ident,
	Path,
	Token,
	TraitItemMethod,
	TypeParamBound,
	Visibility,
};

pub mod attribute;
pub mod dyntrait;

pub use attribute::AttributeOptions;

/// Collection of DynTrait body tokens.
#[derive(Debug)]
pub struct DynTraitBody {
	pub attrs: Vec<Attribute>,
	pub vis: Visibility,
	pub unsafety: Option<Token![unsafe]>,
	pub trait_token: Token![trait],
	pub ident: Ident,
	pub generics: Generics,
	pub colon_token: Option<Token![:]>,
	pub supertraits: Punctuated<TypeParamBound, Token![+]>,
	pub brace_token: token::Brace,
	pub entries: Vec<VTableEntry>,
}

/// DynTrait VTable entry
#[derive(Debug)]
pub enum VTableEntry {
	/// # Note:
	/// Subtables are represented as VTable entries
	/// instead of a seperate list to allow positioning
	/// them differently within a DynTrait's VTable if
	/// another representation is added which allows that (struct form)
	Subtable(SubtableEntry),
	Method(TraitItemMethod),
}

/// A VTable's direct subtable, to be used as
/// a vtable entry.
#[derive(Debug)]
pub struct SubtableEntry {
	pub ident: Ident,
	pub subtable: Subtable,
}

/// A direct or inherited subtable of a different VTable.
///
/// # Note:
/// A Supertrait is a trait bound, either directly specified
/// or inherited from a direct supertrait.
/// A Subtable is a VTable embedded in another VTable.
///
/// The word used will be whichever one makes more sense in
/// context, but may be understood as the same thing.
#[derive(Debug)]
pub struct Subtable {
	pub path: Path,
	/// # Note:
	/// The subtable graph is resolved in parsing as it
	/// may be represented differently depending on the
	/// form of the annotated item.
	pub subtables: Vec<Subtable>,
}

impl Parse for DynTraitBody {
	fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
		// currently the only structure that can be annotated
		// is a trait.
		dyntrait::parse_trait(input)
	}
}

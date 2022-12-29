use proc_macro2::Span;
use syn::{
	parse::ParseStream,
	punctuated::Punctuated,
	spanned::Spanned,
	token,
	Generics,
	Ident,
	LitStr,
	Path,
	Token,
	TraitItemMethod,
	TypeParamBound,
	Visibility,
};

pub mod attribute;
pub mod dyntrait;

use self::{attribute::AttributeOptions, dyntrait::DynTraitBody};

#[derive(Debug)]
pub struct DynTraitInfo {
	pub vis: Visibility,
	pub unsafety: Option<Token![unsafe]>,
	pub vtable: VTableInfo,
	pub dyntrait: TraitInfo,
	pub drop: Option<Abi>,
	pub relax_abi: bool,
	pub generics: Generics,
	pub entries: Vec<VTableEntry>,
}

#[derive(Debug)]
pub struct VTableInfo {
	pub repr: Abi,
	pub name: Ident,
}

#[derive(Debug)]
pub struct TraitInfo {
	pub ident: Ident,
	pub trait_token: Token![trait],
	pub colon_token: Option<Token![:]>,
	pub supertraits: Punctuated<TypeParamBound, Token![+]>,
	pub brace_token: token::Brace,
}

#[derive(Debug)]
pub enum Abi {
	ImplicitRust,
	Explicit(Ident),
}

impl Abi {
	fn new_explicit_c() -> Abi {
		Self::Explicit(Ident::new("C", Span::call_site()))
	}

	/// Structs are not allowed to have an explicit repr, so
	/// an explicitly specified rust repr will be converted to
	/// an implicit one.
	fn parse_struct_repr(input: ParseStream) -> syn::Result<Abi> {
		let abi = input.parse::<Ident>()?;

		Ok(match &abi.to_string() as &str {
			"Rust" => Abi::ImplicitRust,
			_ => Self::Explicit(abi),
		})
	}

	pub fn as_abi(&self) -> Option<syn::Abi> {
		match self {
			Self::ImplicitRust => None,
			Self::Explicit(abi) => Some(syn::Abi {
				extern_token: Default::default(),
				name: Some(LitStr::new(&abi.to_string(), abi.span())),
			}),
		}
	}
}

/// DynTrait VTable entry
#[derive(Debug)]
pub enum VTableEntry {
	// Note:
	// Subtables are represented as VTable entries
	// instead of a seperate list to allow positioning
	// them differently within a DynTrait's VTable if
	// another representation is added which allows that (struct form)
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
	// Note:
	// The subtable graph is resolved in parsing as it
	// may be represented differently depending on the
	// form of the annotated item.
	pub subtables: Vec<Subtable>,
}

impl DynTraitInfo {
	pub fn parse_trait(
		attr: proc_macro::TokenStream,
		item: proc_macro::TokenStream,
	) -> syn::Result<Self> {
		let attr_options = syn::parse::<AttributeOptions>(attr)?;
		let trait_body = syn::parse::<DynTraitBody>(item)?;

		if !attr_options.relax_abi {
			for method in &trait_body.methods {
				if method.sig.abi.is_none() {
					return Err(syn::Error::new(
						method.sig.fn_token.span(),
						"missing explicit ABI specifier (add `relax_abi = true` to the #[dyntrait] annotation to relax this check)",
					))
				}
			}
		}

		Ok(Self {
			vis: trait_body.vis,
			unsafety: trait_body.unsafety,
			vtable: VTableInfo {
				repr: attr_options.repr,
				name: Ident::new(
					// VTable name's span should match trait name's span
					&match attr_options.vtable_name {
						Some(ident) => ident.to_string(),
						None => format!("{}VTable", &trait_body.ident.to_string()),
					},
					trait_body.ident.span(),
				),
			},
			dyntrait: TraitInfo {
				ident: trait_body.ident,
				trait_token: trait_body.trait_token,
				colon_token: trait_body.colon_token,
				supertraits: trait_body.supertraits,
				brace_token: trait_body.brace_token,
			},
			drop: attr_options.drop,
			relax_abi: attr_options.relax_abi,
			generics: trait_body.generics,
			entries: trait_body
				.subtables
				.into_iter()
				.map(VTableEntry::Subtable)
				.chain(trait_body.methods.into_iter().map(VTableEntry::Method))
				.collect(),
		})
	}
}

/// Subtable parent-child relation
pub struct SubtableChildGraph<'a> {
	pub parent: &'a Subtable,
	pub child: &'a Subtable,
}

impl Subtable {
	pub fn flatten<'s>(&'s self) -> Vec<&'s Self> {
		let mut subtables = Vec::<&'s Self>::new();
		self.flatten_into(&mut subtables);
		subtables
	}

	fn flatten_into<'s>(&'s self, subtables: &mut Vec<&'s Self>) {
		subtables.push(&self);

		for subtable in &self.subtables {
			subtable.flatten_into(subtables);
		}
	}

	/// Flatten subtables of this subtable into a parent-child relation
	pub fn flatten_child_graph<'s>(&'s self) -> Vec<SubtableChildGraph<'s>> {
		let mut subtables = Vec::<SubtableChildGraph>::new();
		self.flatten_into_child_graph(&mut subtables);
		subtables
	}

	/// See `flatten_child_graph`
	fn flatten_into_child_graph<'s>(&'s self, subtables: &mut Vec<SubtableChildGraph<'s>>) {
		for subtable in &self.subtables {
			subtables.push(SubtableChildGraph {
				parent: &self,
				child: subtable,
			});

			subtable.flatten_into_child_graph(subtables);
		}
	}
}

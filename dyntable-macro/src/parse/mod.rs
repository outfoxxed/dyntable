use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::{
	parse::ParseStream,
	punctuated::Punctuated,
	spanned::Spanned,
	token,
	Attribute,
	Generics,
	Ident,
	Lifetime,
	LitStr,
	Path,
	ReturnType,
	Token,
	Type,
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
	pub attrs: Vec<Attribute>,
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

#[derive(Debug)]
pub enum PointerType {
	Const(Token![const]),
	Mut(Token![mut]),
}

impl From<Option<Token![mut]>> for PointerType {
	fn from(value: Option<Token![mut]>) -> Self {
		match value {
			Some(value) => Self::Mut(value),
			None => Self::Const(Default::default()),
		}
	}
}

impl ToTokens for PointerType {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		match self {
			Self::Const(token) => token.to_tokens(tokens),
			Self::Mut(token) => token.to_tokens(tokens),
		}
	}
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

	pub fn as_repr(&self) -> Option<TokenStream> {
		match self {
			Self::ImplicitRust => None,
			Self::Explicit(abi) => Some(quote::quote! {
				#[repr(#abi)]
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
	Method(MethodEntry),
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

#[derive(Debug)]
pub struct MethodEntry {
	pub unsafety: Option<Token![unsafe]>,
	pub abi: Option<syn::Abi>,
	pub fn_token: Token![fn],
	pub ident: Ident,
	pub generics: Generics,
	pub receiver: MethodReceiver,
	/// # Note
	/// does not include receiver
	pub inputs: Vec<MethodParam>,
	pub output: ReturnType,
}

#[derive(Debug)]
pub enum MethodReceiver {
	Reference(ReceiverReference),
	Value(Token![self]),
}

#[derive(Debug)]
pub struct ReceiverReference {
	pub reference: (Token![&], Option<Lifetime>),
	pub mutability: Option<Token![mut]>,
	pub self_token: Token![self],
}

impl MethodReceiver {
	/// Get the pointer type (mut / const) for this receiver.
	///
	/// # Note
	/// An owned receiver will be `mut` due to it being passed
	/// to the shim as a pointer.
	pub fn pointer_type(&self) -> PointerType {
		match self {
			Self::Value(_) => PointerType::Mut(Default::default()),
			Self::Reference(ReceiverReference { mutability, .. }) => PointerType::from(*mutability),
		}
	}
}

impl ToTokens for ReceiverReference {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		self.reference.0.to_tokens(tokens);
		self.reference.1.to_tokens(tokens);
		self.mutability.to_tokens(tokens);
		self.self_token.to_tokens(tokens);
	}
}

impl ToTokens for MethodReceiver {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		match self {
			Self::Value(x) => x.to_tokens(tokens),
			Self::Reference(x) => x.to_tokens(tokens),
		}
	}
}

#[derive(Debug)]
pub struct MethodParam {
	pub ident: Ident,
	pub colon_token: Token![:],
	pub ty: Type,
}

impl MethodParam {
	/// List safe parameter names that will work regardless of the ident.
	/// This allows `_` parameter names.
	pub fn idents_safe<'s>(
		inputs: impl Iterator<Item = &'s Self> + 's,
	) -> impl Iterator<Item = Ident> + 's {
		inputs
			.enumerate()
			.map(|(i, Self { ident, .. })| Ident::new(&format!("arg{i}"), ident.span()))
	}

	/// List safe names that will work regardless of the ident.
	pub fn params_safe<'s>(
		inputs: impl Iterator<Item = &'s Self> + 's,
	) -> impl Iterator<Item = Self> + 's {
		inputs.enumerate().map(
			|(
				i,
				Self {
					ident,
					colon_token,
					ty,
				},
			)| Self {
				ident: Ident::new(&format!("arg{i}"), ident.span()),
				colon_token: *colon_token,
				ty: ty.clone(),
			},
		)
	}
}

impl ToTokens for MethodParam {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		self.ident.to_tokens(tokens);
		self.colon_token.to_tokens(tokens);
		self.ty.to_tokens(tokens);
	}
}

impl ToTokens for MethodEntry {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let Self {
			unsafety,
			abi,
			fn_token,
			ident,
			generics,
			receiver,
			inputs,
			output,
		} = self;

		let (_, ty_generics, where_clause) = generics.split_for_impl();

		tokens.extend(quote::quote! {
			#unsafety #abi #fn_token #ident #ty_generics (#receiver, #(#inputs),*) #output
			#where_clause;
		});
	}
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
				if method.abi.is_none() {
					return Err(syn::Error::new(
						method.fn_token.span(),
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
				attrs: trait_body.attrs,
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

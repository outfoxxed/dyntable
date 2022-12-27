//! VTable generation code

use proc_macro2::{Ident, Span, TokenStream};
use quote::ToTokens;
use syn::{
	punctuated::Punctuated,
	spanned::Spanned,
	token::Paren,
	AttrStyle,
	Attribute,
	BareFnArg,
	Field,
	Fields,
	FieldsNamed,
	FnArg,
	ItemStruct,
	PatType,
	Path,
	ReturnType,
	Signature,
	TraitItemMethod,
	Type,
	TypeBareFn,
	TypePath,
	TypePtr,
	TypeReference,
	VisPublic,
	Visibility,
};

use super::absolute_path;
use crate::parse::{DynTraitInfo, Subtable, SubtableEntry, VTableEntry};

/// Build a VTable from the information in a [`DynTraitBody`]
pub fn build_vtable(trait_body: &DynTraitInfo) -> syn::Result<ItemStruct> {
	let vtable_entries = trait_body
		.entries
		.iter()
		.map(|entry| {
			Ok::<_, syn::Error>(match entry {
				VTableEntry::Subtable(SubtableEntry {
					ident,
					subtable: Subtable { path, .. },
				}) => Field {
					attrs: Vec::new(),
					vis: Visibility::Public(VisPublic {
						pub_token: Default::default(),
					}),
					colon_token: Some(Default::default()),
					ident: Some(ident.clone()),
					ty: Type::Path(TypePath {
						qself: None,
						path: path.clone(),
					}),
				},
				VTableEntry::Method(TraitItemMethod {
					sig:
						Signature {
							unsafety,
							abi,
							fn_token,
							ident,
							paren_token,
							inputs,
							output,
							..
						},
					..
				}) => Field {
					attrs: Vec::new(),
					vis: Visibility::Public(VisPublic {
						pub_token: Default::default(),
					}),
					colon_token: Some(Default::default()),
					ident: Some(ident.clone()),
					ty: Type::BareFn(TypeBareFn {
						lifetimes: None, // TODO: See TODO entry for strip_references
						unsafety: unsafety.clone(),
						abi: abi.clone(),
						fn_token: fn_token.clone(),
						paren_token: paren_token.clone(),
						inputs: map_fn_arguments(inputs.clone()).collect::<Result<_, _>>()?,
						variadic: None, // TODO: possible future support for variadics
						output: match output {
							ReturnType::Default => ReturnType::Default,
							ReturnType::Type(arrow, ty) => ReturnType::Type(
								arrow.clone(),
								Box::new(strip_references(ty.as_ref().clone())),
							),
						},
					}),
				},
			})
		})
		.collect::<Result<Punctuated<_, _>, _>>()?;

	// TODO: add #[repr] when applicable
	let attributes = vec![Attribute {
		pound_token: Default::default(),
		style: AttrStyle::Outer,
		bracket_token: Default::default(),
		path: Path::from(Ident::new("allow", Span::call_site())),
		tokens: {
			let mut tokens = TokenStream::new();

			Paren {
				span: Span::call_site(),
			}
			.surround(&mut tokens, |tokens| {
				Path::from(Ident::new("non_snake_case", Span::call_site())).to_tokens(tokens)
			});

			tokens
		},
	}];

	Ok(ItemStruct {
		attrs: attributes,
		vis: syn::Visibility::Inherited,
		struct_token: Default::default(),
		ident: trait_body.dyntrait.ident.clone(),
		semi_token: None,
		fields: Fields::Named(FieldsNamed {
			brace_token: Default::default(),
			named: vtable_entries,
		}),
		generics: trait_body.generics.clone(),
	})
}

/// Map [`FnArg`]s to [`BareFnArg`]s, replacing toplevel
/// references with raw pointers.
// See TODO entry for strip_references
fn map_fn_arguments(
	argument_stream: impl IntoIterator<Item = FnArg>,
) -> impl Iterator<Item = syn::Result<BareFnArg>> {
	argument_stream.into_iter().map(|argument| {
		Ok(match argument {
			FnArg::Receiver(receiver) => {
				// TODO: experiment with allowing an owned receiver by
				// moving an owned pointer to the stack, and a
				// `Box<Self>` with Box::from_raw
				if receiver.reference.is_none() {
					return Err(syn::Error::new(
						receiver.span(),
						"methods cannot take an self by value in #[dyntable] annotated traits",
					))
				}

				BareFnArg {
					attrs: Vec::new(),
					name: None,
					ty: Type::Path(TypePath {
						qself: None,
						path: absolute_path(["core", "ffi", "c_void"]),
					}),
				}
			},
			FnArg::Typed(PatType { ty, .. }) => BareFnArg {
				attrs: Vec::new(),
				name: None,
				ty: strip_references(*ty),
			},
		})
	})
}

/// Replace toplevel references in a [`Type`] with raw pointers
// TODO: reassess how nessesary this is and if it could be a
// source of UB. At this point the main goal is to copy the old
// macro's functionality (toplevel references to pointers).
fn strip_references(ty: Type) -> Type {
	match ty {
		Type::Reference(TypeReference {
			mutability, elem, ..
		}) => Type::Ptr(TypePtr {
			star_token: Default::default(),
			const_token: match &mutability {
				Some(_) => None,
				None => Some(Default::default()),
			},
			mutability,
			// TODO: add tests to check if nested references need
			// to be removed (if they need to be removed at all, see above todo)
			elem,
		}),
		other => other,
	}
}

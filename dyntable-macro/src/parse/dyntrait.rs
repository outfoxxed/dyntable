//! Parsing structures for a `#[dyntable]` annotated trait.

use std::{
	collections::{HashMap, HashSet},
	mem,
};

use proc_macro2::{Span, TokenStream};
use quote::ToTokens;
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	spanned::Spanned,
	token,
	Attribute,
	FnArg,
	GenericParam,
	Generics,
	Ident,
	ItemTrait,
	LifetimeParam,
	Pat,
	PatIdent,
	PatType,
	PatWild,
	Path,
	PredicateType,
	Receiver,
	Signature,
	Token,
	TraitBound,
	TraitBoundModifier,
	TraitItem,
	TraitItemFn,
	TraitItemType,
	Type,
	TypeParamBound,
	TypeTraitObject,
	Visibility,
	WhereClause,
	WherePredicate,
};

use super::{MethodEntry, MethodParam, MethodReceiver, ReceiverReference, Subtable};
use crate::parse::SubtableEntry;

/// Validated #[dyntable] trait AST tokens
#[derive(Debug)]
pub struct DynTraitBody {
	pub attrs: Vec<Attribute>,
	pub vis: Visibility,
	pub unsafety: Option<Token![unsafe]>,
	pub trait_token: Token![trait],
	pub ident: Ident,
	pub generics: Generics,
	pub associated_types: Vec<TraitItemType>,
	pub colon_token: Option<Token![:]>,
	pub supertraits: Punctuated<TypeParamBound, Token![+]>,
	pub subtables: Vec<SubtableEntry>,
	pub brace_token: token::Brace,
	pub methods: Vec<MethodEntry>,
}

impl Parse for DynTraitBody {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let mut dyntrait = input.parse::<ItemTrait>()?;

		let dyn_entries = match &mut dyntrait.generics.where_clause {
			Some(where_clause) => strip_dyn_entries(where_clause)?,
			None => Vec::new(),
		};

		let subtables = solve_subtables(
			dyntrait.supertraits.iter().filter_map(|bound| match bound {
				TypeParamBound::Trait(t) => Some(t.clone()),
				_ => None,
			}),
			dyn_entries.into_iter(),
		)?;

		let subtable_entries = {
			// Keeps track of the number of times a "friendly name" of a trait
			// is used to avoid having to use name mangling.
			let mut used_vtable_names = HashMap::<String, u32>::new();

			subtables
				.into_iter()
				.map(|subtable| {
					// "friendly name" of a trait.
					// `foo::Bar<Baz>` becomes `Bar`
					let bound_name = subtable.path.segments.last()
						// under what circumstances would you type `trait MyTrait: :: {}`
						.ok_or_else(|| syn::Error::new_spanned(
							&subtable.path,
							"not a valid path",
						))?
						.ident
						.to_string();

					let name = match used_vtable_names.get(&bound_name) {
						Some(&(mut count)) => {
							count += 1;
							let entry_name = format!("__vtable_{}{}", &bound_name, count);
							used_vtable_names.insert(bound_name, count);
							entry_name
						},
						None => {
							let entry_name = format!("__vtable_{}", &bound_name);
							used_vtable_names.insert(bound_name, 1);
							entry_name
						},
					};

					Ok(SubtableEntry {
						ident: Ident::new(&name, Span::call_site()),
						subtable,
					})
				})
				.collect::<syn::Result<Vec<_>>>()?
		};

		let mut methods = Vec::<MethodEntry>::new();
		let mut associated_types = Vec::<TraitItemType>::new();

		for item in dyntrait.items {
			match item {
				TraitItem::Type(item) => associated_types.push(item),
				TraitItem::Fn(TraitItemFn { sig, .. }) => methods.push(MethodEntry::try_from(sig)?),

				TraitItem::Const(entry) => return Err(syn::Error::new_spanned(entry, "associated constants are not supported in #[dyntable] traits")),
				TraitItem::Macro(entry) => return Err(syn::Error::new_spanned(entry, "macro invocations are not supported for directly creating entries in #[dyntable] traits")),
				entry => return Err(syn::Error::new_spanned(entry, "unknown entry")),
			}
		}

		Ok(Self {
			attrs: dyntrait.attrs,
			vis: dyntrait.vis,
			unsafety: dyntrait.unsafety,
			trait_token: dyntrait.trait_token,
			ident: dyntrait.ident,
			generics: dyntrait.generics,
			colon_token: dyntrait.colon_token,
			supertraits: dyntrait.supertraits,
			subtables: subtable_entries,
			brace_token: dyntrait.brace_token,
			methods,
			associated_types,
		})
	}
}

/// Strip and return dyn entries from a where clause
fn strip_dyn_entries(where_clause: &mut WhereClause) -> Result<Vec<DynPredicate>, syn::Error> {
	let mut dyn_entries = Vec::<DynPredicate>::new();
	let mut where_predicates = Punctuated::<WherePredicate, Token![,]>::new();
	mem::swap(&mut where_predicates, &mut where_clause.predicates);

	for predicate in where_predicates {
		// purposefully avoids `Type::Paren`, which acts as an escape hatch
		if let WherePredicate::Type(PredicateType {
			bounded_ty:
				Type::TraitObject(
					traitobj @ TypeTraitObject {
						dyn_token: Some(_), ..
					},
				),
			..
		}) = &predicate
		{
			let traitobj_span = traitobj.span();

			let WherePredicate::Type(PredicateType {
				bounded_ty: Type::TraitObject(TypeTraitObject { dyn_token: Some(dyn_token), bounds }),
				lifetimes,
				colon_token,
				bounds: predicate_bounds,
			}) = predicate else {
				unreachable!("pattern matched on reference immediately before destructure")
			};

			let bounded_ty = {
				let mut iter = bounds.into_iter();
				let bound = iter.next().ok_or_else(|| {
					syn::Error::new(traitobj_span, "dyn bound must have exactly 1 trait")
				})?;

				if let Some(extra_trait) = iter.next() {
					return Err(syn::Error::new_spanned(
						extra_trait,
						"dyn bound must have exactly 1 trait",
					))
				}

				match bound {
					TypeParamBound::Trait(TraitBound {
						modifier: TraitBoundModifier::None,
						lifetimes: None,
						path,
						..
					}) => path,
					TypeParamBound::Trait(TraitBound {
						lifetimes: Some(bound_lifetimes),
						..
					}) => {
						return Err(syn::Error::new_spanned(
							bound_lifetimes,
							"dyn bound cannot have higher ranked trait bounds",
						))
					},
					TypeParamBound::Trait(TraitBound { modifier, .. }) => {
						return Err(syn::Error::new_spanned(
							modifier,
							"dyn bound cannot have trait modifier",
						))
					},
					bound => {
						return Err(syn::Error::new_spanned(bound, "dyn bound must be a trait"))
					},
				}
			};

			if let Some(bound_lifetimes) = lifetimes {
				return Err(syn::Error::new_spanned(
					bound_lifetimes,
					"dyn bound cannot have higher ranked trait bounds",
				))
			}

			let bounds = {
				let mut bounds = Punctuated::<Path, Token![+]>::new();

				for bound in predicate_bounds {
					match bound {
						TypeParamBound::Trait(TraitBound {
							modifier: TraitBoundModifier::None,
							lifetimes: None,
							path,
							..
						}) => bounds.push(path),
						TypeParamBound::Trait(TraitBound {
							lifetimes: Some(bound_lifetimes),
							..
						}) => {
							return Err(syn::Error::new_spanned(
								bound_lifetimes,
								"dyn bound cannot have higher ranked trait bounds",
							))
						},
						TypeParamBound::Trait(TraitBound { modifier, .. }) => {
							return Err(syn::Error::new_spanned(
								modifier,
								"dyn bound cannot have trait modifier",
							))
						},
						bound => {
							return Err(syn::Error::new_spanned(bound, "dyn bound must be a trait"))
						},
					}
				}

				bounds
			};

			dyn_entries.push(DynPredicate {
				dyn_token,
				bounded_ty,
				colon_token,
				bounds,
			});
		} else {
			where_clause.predicates.push(predicate);
		}
	}

	Ok(dyn_entries)
}

pub struct DynPredicate {
	pub dyn_token: Token![dyn],
	pub bounded_ty: Path,
	pub colon_token: Token![:],
	pub bounds: Punctuated<Path, Token![+]>,
}

/// Build a list of subtable graphs from trait bounds
///
/// # Note:
/// For the disambiguation of supertrait/subtable,
/// see the rustdoc for [`Subtable`].
fn solve_subtables(
	trait_bounds: impl Iterator<Item = TraitBound>,
	where_predicates: impl Iterator<Item = DynPredicate>,
) -> syn::Result<Vec<Subtable>> {
	let mut supertrait_map = HashMap::<Path, Option<Punctuated<Path, Token![+]>>>::new();
	// paths can only be specified once
	let mut specified_paths = HashSet::<Path>::new();

	// Create a supertrait -> bound list mapping of all
	// specified dyn supertraits in a where clause
	for DynPredicate {
		bounded_ty, bounds, ..
	} in where_predicates
	{
		match supertrait_map.get(&bounded_ty) {
			None => {
				for bound in &bounds {
					if specified_paths.contains(bound) {
						return Err(syn::Error::new_spanned(
							bound,
							"exactly one path to an inherited trait bound must be specified (this path is a duplicate)",
						))
					} else {
						specified_paths.insert(bound.clone());
					}
				}

				supertrait_map.insert(bounded_ty, Some(bounds));
			},
			Some(_) => {
				return Err(syn::Error::new_spanned(
					bounded_ty,
					"supertrait already listed",
				))
			},
		}
	}

	let mut subtables = Vec::<Subtable>::new();
	// keeps track of used entries to disallow unused entries
	let mut used_supertrait_entries = HashSet::<Path>::new();

	// Create subtable graphs for all trait bounds with
	// dyn entries in the where clause.
	for TraitBound { path, .. } in trait_bounds {
		if supertrait_map.contains_key(&path) {
			subtables.push(graph_subtables(
				path,
				&supertrait_map,
				&mut used_supertrait_entries,
			));
		}
	}

	// Return an error if a where clause dyn entry is not
	// referenced directly or indirectly from a trait bound.
	for supertrait in supertrait_map.keys() {
		if !used_supertrait_entries.contains(supertrait) {
			return Err(syn::Error::new_spanned(
				supertrait,
				"dyn trait bound has no relation to the defined trait. dyn bounds must match a direct trait bound or indirect trait bound (through a different dyn bound)",
			))
		}
	}

	Ok(subtables)
}

/// Recursively graph a dyntrait's supertraits into
/// a list of nested subtable paths
///
/// # Note:
/// This is a function and not a closure because closures
/// cannot be called recursively.
///
/// Example:
///
/// ```text
/// where
///     dyn A: B + C,
///     dyn B: D,
/// ```
///
/// is converted to
///
/// ```text
/// Subtable {
/// 	ident: "A",
/// 	subtables: [
/// 		Subtable {
/// 			ident: "B",
/// 			subtables: [Subtable {
/// 				ident: "D",
/// 				subtables: [],
/// 			}],
/// 		},
/// 		Subtable {
/// 			ident: "C",
/// 			subtables: [],
/// 		},
/// 	],
/// }
/// ```
fn graph_subtables(
	path: Path,
	supertrait_map: &HashMap<Path, Option<Punctuated<Path, syn::token::Plus>>>,
	used_supertrait_entries: &mut HashSet<Path>,
) -> Subtable {
	let mut subtables = Vec::<Subtable>::new();

	if let Some(Some(supertraits)) = supertrait_map.get(&path) {
		for supertrait in supertraits {
			subtables.push(graph_subtables(
				supertrait.clone(),
				supertrait_map,
				used_supertrait_entries,
			));
		}
	}

	used_supertrait_entries.insert(path.clone());

	Subtable { path, subtables }
}

impl TryFrom<Signature> for MethodEntry {
	type Error = syn::Error;

	fn try_from(sig: Signature) -> Result<Self, Self::Error> {
		let sig_span = sig.span();
		let Signature {
			unsafety,
			abi,
			fn_token,
			ident,
			generics,
			inputs,
			variadic,
			output,
			..
		} = sig;

		if let Some(variadic) = variadic {
			return Err(syn::Error::new_spanned(
				variadic,
				"variadics are not supported in #[dyntable] annotated traits",
			))
		}

		if let Some(where_clause) = generics.where_clause {
			return Err(syn::Error::new_spanned(
				where_clause,
				"where clauses cannot be specified for functions in #[dyntable] traits",
			))
		}

		for param in &generics.params {
			match param {
				GenericParam::Lifetime(LifetimeParam {
					colon_token: Some(colon_tok),
					bounds,
					..
				}) => {
					let mut buffer = TokenStream::new();
					colon_tok.to_tokens(&mut buffer);
					bounds.to_tokens(&mut buffer);
					return Err(syn::Error::new_spanned(buffer, "lifetime bounds cannot be specified for function lifetime generics in #[dyntable] traits"))
				},
				GenericParam::Lifetime(_) => {}, // non-bounded lifetimes are fine
				GenericParam::Const(param) => {
					return Err(syn::Error::new_spanned(
						param,
						"const generics cannot be specified for functions in #[dyntable] traits",
					))
				},
				GenericParam::Type(param) => {
					return Err(syn::Error::new_spanned(
						param,
						"type generics cannot be specified for functions in #[dyntable] traits",
					))
				},
			}
		}

		let mut receiver = Option::<MethodReceiver>::None;
		let mut args = Vec::<MethodParam>::with_capacity(inputs.len().saturating_sub(1));

		for input in inputs {
			match input {
				FnArg::Receiver(
					receiver @ Receiver {
						colon_token: Some(_),
						..
					},
				) => {
					// explicitly typed self makes it impossible to determine if the
					// type is a reference or value, due to type aliases and permitted
					// wrappers.
					return Err(syn::Error::new_spanned(receiver, "`self` parameter must use implicit type syntax (e.g. `self`, `&self`, `&mut self`)"));
				},
				FnArg::Receiver(Receiver {
					reference: None,
					self_token,
					..
				}) => {
					if receiver.is_some() {
						return Err(syn::Error::new_spanned(
							self_token,
							"`self` is bound more than once",
						))
					}

					receiver = Some(MethodReceiver::Value(self_token));
				},
				FnArg::Receiver(Receiver {
					reference: Some(reference),
					mutability,
					self_token,
					..
				}) => {
					if receiver.is_some() {
						return Err(syn::Error::new_spanned(
							self_token,
							"`self` is bound more than once",
						))
					}

					receiver = Some(MethodReceiver::Reference(ReceiverReference {
						reference,
						mutability,
						self_token,
					}));
				},
				FnArg::Typed(ty) => {
					let PatType {
						pat,
						ty,
						colon_token,
						..
					} = ty;

					let pat_span = pat.span();

					let ident = match *pat {
						Pat::Ident(PatIdent {
							by_ref: None,
							mutability: None,
							subpat: None,
							ident,
							..
						}) => ident,
						Pat::Wild(PatWild {
							underscore_token, ..
						}) => Ident::new("_", underscore_token.span()),
						pat => {
							return Err(syn::Error::new_spanned(
								pat,
								"patterns are not supported in dyntrait methods",
							))
						},
					};

					if receiver.is_none() {
						return Err(syn::Error::new(pat_span, "first parameter must be `self`"))
					}

					args.push(MethodParam {
						ident,
						colon_token,
						ty: *ty,
					});
				},
			}
		}

		let receiver = receiver
			.ok_or_else(|| syn::Error::new(sig_span, "missing required `self` parameter"))?;

		Ok(Self {
			unsafety,
			abi,
			fn_token,
			ident,
			generics,
			receiver,
			inputs: args,
			output,
		})
	}
}

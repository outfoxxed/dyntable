//! Parsing structures for a `#[dyntable]` annotated trait.

use std::collections::{HashMap, HashSet};

use proc_macro2::Span;
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	spanned::Spanned,
	token,
	Attribute,
	FnArg,
	Generics,
	Ident,
	Pat,
	PatIdent,
	PatType,
	Path,
	Receiver,
	Signature,
	Token,
	TraitBound,
	TraitItem,
	TraitItemMethod,
	TypeParamBound,
	Visibility,
};

use self::parse::{DynTrait, DynWherePredicateSupertrait};
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
	pub colon_token: Option<Token![:]>,
	pub supertraits: Punctuated<TypeParamBound, Token![+]>,
	pub subtables: Vec<SubtableEntry>,
	pub brace_token: token::Brace,
	pub methods: Vec<MethodEntry>,
}

impl Parse for DynTraitBody {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let dyntrait = input.parse::<DynTrait>()?;

		let subtables = solve_subtables(
			dyntrait.supertraits.iter().filter_map(|bound| match bound {
				TypeParamBound::Trait(t) => Some(t.clone()),
				_ => None,
			}),
			dyntrait
				.generics
				.where_clause
				.as_ref()
				.map(|where_clause| where_clause.predicates.clone())
				.unwrap_or_else(|| Punctuated::new())
				.into_iter()
				.filter_map(|predicate| match predicate {
					parse::DynWherePredicate::Dyn(predicate) => Some(predicate),
					_ => None,
				}),
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
						// under what circumstanced would you type `trait MyTrait: :: {}`
						.ok_or_else(|| syn::Error::new(
							subtable.path.span(),
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

		let methods = dyntrait
			.items
			.into_iter()
			.map(|item| match item {
				TraitItem::Method(TraitItemMethod { sig, .. }) => Ok(MethodEntry::try_from(sig)?),
				item => Err(syn::Error::new(
					item.span(),
					"only method defintions are allowed in #[dyntable] annotated traits",
				)),
			})
			.collect::<syn::Result<Vec<_>>>()?;

		Ok(Self {
			attrs: dyntrait.attrs,
			vis: dyntrait.vis,
			unsafety: dyntrait.unsafety,
			trait_token: dyntrait.trait_token,
			ident: dyntrait.ident,
			generics: dyntrait.generics.strip_dyntable(),
			colon_token: dyntrait.colon_token,
			supertraits: dyntrait.supertraits,
			subtables: subtable_entries,
			brace_token: dyntrait.brace_token,
			methods,
		})
	}
}

/// Build a list of subtable graphs from trait bounds
///
/// # Note:
/// For the disambiguation of supertrait/subtable,
/// see the rustdoc for [`Subtable`].
fn solve_subtables(
	trait_bounds: impl Iterator<Item = TraitBound>,
	where_predicates: impl Iterator<Item = DynWherePredicateSupertrait>,
) -> syn::Result<Vec<Subtable>> {
	let mut supertrait_map = HashMap::<Path, Option<Punctuated<Path, Token![+]>>>::new();
	// paths can only be specified once
	let mut specified_paths = HashSet::<Path>::new();

	// Create a supertrait -> bound list mapping of all
	// specified dyn supertraits in a where clause
	for DynWherePredicateSupertrait {
		bounded_ty, bounds, ..
	} in where_predicates
	{
		match supertrait_map.get(&bounded_ty) {
			None => {
				for bound in &bounds {
					if specified_paths.contains(bound) {
						return Err(syn::Error::new(
							bound.span(),
							"exactly one path to an inherited trait bound must be specified (this path is a duplicate)",
						))
					} else {
						specified_paths.insert(bound.clone());
					}
				}

				supertrait_map.insert(bounded_ty, Some(bounds));
			},
			Some(_) => {
				return Err(syn::Error::new(
					bounded_ty.span(),
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
			return Err(syn::Error::new(
				supertrait.span(),
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
/// ```
/// where
///     dyn A: B + C,
///     dyn B: D,
/// ```
/// is converted to
/// ```
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
	supertrait_map: &HashMap<Path, Option<Punctuated<Path, syn::token::Add>>>,
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
			return Err(syn::Error::new(
				variadic.span(),
				"variadics are not supported in #[dyntable] annotated traits",
			))
		}

		let mut receiver = Option::<MethodReceiver>::None;
		let mut args = Vec::<MethodParam>::with_capacity(inputs.len().saturating_sub(1));

		for input in inputs {
			match input {
				FnArg::Receiver(Receiver {
					reference: None,
					self_token,
					..
				}) => {
					if receiver.is_some() {
						return Err(syn::Error::new(
							self_token.span(),
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
						return Err(syn::Error::new(
							self_token.span(),
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
					let Pat::Ident(PatIdent {
						by_ref: None,
						mutability: None,
						subpat: None,
						ident,
						..
					}) = *pat else {
						return Err(syn::Error::new(pat.span(), "patterns are not supported in dyntrait methods"))
					};

					if receiver.is_none() {
						if ident.to_string() == "self" {
							// explicitly typed self makes it impossible to determine if the
							// type is a reference or value, due to type aliases and permitted
							// wrappers.
							return Err(syn::Error::new(ident.span(), "`self` parameter must use implicit type syntax (e.g. `self`, `&self`, `&mut self`)"));
						} else {
							return Err(syn::Error::new(pat_span, "first parameter must be `self`"))
						}
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

// copied from old trait parsing code.
// TODO: review
mod parse {
	use syn::{
		braced,
		ext::IdentExt,
		parse::{Parse, ParseStream},
		punctuated::Punctuated,
		token::{self, Trait},
		Attribute,
		ConstParam,
		GenericParam,
		Generics,
		Ident,
		Lifetime,
		LifetimeDef,
		Path,
		PredicateEq,
		PredicateLifetime,
		PredicateType,
		Token,
		TraitItem,
		TypeParam,
		TypeParamBound,
		Visibility,
		WhereClause,
		WherePredicate,
	};

	#[derive(Debug, Clone)]
	pub struct DynTrait {
		pub attrs: Vec<Attribute>,
		pub vis: Visibility,
		pub unsafety: Option<Token![unsafe]>,
		pub trait_token: Trait,
		pub ident: Ident,
		pub generics: DynGenerics,
		pub colon_token: Option<Token![:]>,
		pub supertraits: Punctuated<TypeParamBound, Token![+]>,
		pub brace_token: token::Brace,
		pub items: Vec<TraitItem>,
	}

	#[derive(Debug, Clone)]
	pub struct DynGenerics {
		pub lt_token: Option<Token![<]>,
		pub params: Punctuated<GenericParam, Token![,]>,
		pub gt_token: Option<Token![>]>,
		pub where_clause: Option<DynWhereClause>,
	}

	#[derive(Debug, Clone)]
	pub struct DynWhereClause {
		pub where_token: Token![where],
		pub predicates: Punctuated<DynWherePredicate, Token![,]>,
	}

	#[derive(Debug, Clone)]
	pub enum DynWherePredicate {
		Dyn(DynWherePredicateSupertrait),
		Type(PredicateType),
		Lifetime(PredicateLifetime),
		Eq(PredicateEq),
	}

	#[derive(Debug, Clone)]
	pub struct DynWherePredicateSupertrait {
		pub dyn_token: Token![dyn],
		pub bounded_ty: Path,
		pub colon_token: Token![:],
		pub bounds: Punctuated<Path, Token![+]>,
	}

	impl DynGenerics {
		/// Strips out all dyntable information, leaving a normal `Generics` struct
		pub fn strip_dyntable(self) -> Generics {
			let Self {
				lt_token,
				params,
				gt_token,
				where_clause,
			} = self;

			Generics {
				lt_token,
				params,
				gt_token,
				where_clause: where_clause.map(|where_clause| where_clause.strip_dyntable()),
			}
		}
	}

	impl DynWhereClause {
		/// Strips out all dyntable information, leaving a normal `WhereClause` struct
		pub fn strip_dyntable(self) -> WhereClause {
			WhereClause {
				where_token: self.where_token,
				predicates: self
					.predicates
					.into_iter()
					.filter_map(|predicate| match predicate {
						DynWherePredicate::Dyn(_) => None,
						DynWherePredicate::Type(x) => Some(WherePredicate::Type(x)),
						DynWherePredicate::Lifetime(x) => Some(WherePredicate::Lifetime(x)),
						DynWherePredicate::Eq(x) => Some(WherePredicate::Eq(x)),
					})
					.collect(),
			}
		}
	}

	impl Default for DynGenerics {
		fn default() -> Self {
			Self {
				lt_token: None,
				params: Punctuated::new(),
				gt_token: None,
				where_clause: None,
			}
		}
	}

	impl From<WherePredicate> for DynWherePredicate {
		fn from(value: WherePredicate) -> Self {
			match value {
				WherePredicate::Type(x) => DynWherePredicate::Type(x),
				WherePredicate::Lifetime(x) => DynWherePredicate::Lifetime(x),
				WherePredicate::Eq(x) => DynWherePredicate::Eq(x),
			}
		}
	}

	impl Parse for DynTrait {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			// copied from <syn::item::TraitItem as Parse>::parse
			let mut attrs = input.call(Attribute::parse_outer)?;
			let vis: Visibility = input.parse()?;
			let unsafety: Option<Token![unsafe]> = input.parse()?;
			let trait_token: Token![trait] = input.parse()?;
			let ident: Ident = input.parse()?;
			let mut generics: DynGenerics = input.parse()?;

			// copied from syn::item::parse_rest_of_trait
			let colon_token: Option<Token![:]> = input.parse()?;

			let mut supertraits = Punctuated::new();
			if colon_token.is_some() {
				loop {
					if input.peek(Token![where]) || input.peek(token::Brace) {
						break
					}
					supertraits.push_value(input.parse()?);
					if input.peek(Token![where]) || input.peek(token::Brace) {
						break
					}
					supertraits.push_punct(input.parse()?);
				}
			}

			generics.where_clause = match input.peek(Token![where]) {
				true => Some(input.parse()?),
				false => None,
			};

			let content;
			let brace_token = braced!(content in input);
			attrs.extend(Attribute::parse_inner(&content)?);
			let mut items = Vec::new();
			while !content.is_empty() {
				items.push(content.parse()?);
			}

			Ok(Self {
				attrs,
				vis,
				unsafety,
				trait_token,
				ident,
				generics,
				colon_token,
				supertraits,
				brace_token,
				items,
			})
		}
	}

	impl Parse for DynGenerics {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			// copied from <syn::generics::Generics as syn::Parse>::parse
			if !input.peek(Token![<]) {
				return Ok(Self::default())
			}

			let lt_token: Token![<] = input.parse()?;

			let mut params = Punctuated::new();
			loop {
				if input.peek(Token![>]) {
					break
				}

				let attrs = input.call(Attribute::parse_outer)?;
				let lookahead = input.lookahead1();
				if lookahead.peek(Lifetime) {
					params.push_value(GenericParam::Lifetime(LifetimeDef {
						attrs,
						..input.parse()?
					}));
				} else if lookahead.peek(Ident) {
					params.push_value(GenericParam::Type(TypeParam {
						attrs,
						..input.parse()?
					}));
				} else if lookahead.peek(Token![const]) {
					params.push_value(GenericParam::Const(ConstParam {
						attrs,
						..input.parse()?
					}));
				} else if input.peek(Token![_]) {
					params.push_value(GenericParam::Type(TypeParam {
						attrs,
						ident: input.call(Ident::parse_any)?,
						colon_token: None,
						bounds: Punctuated::new(),
						eq_token: None,
						default: None,
					}));
				} else {
					return Err(lookahead.error())
				}

				if input.peek(Token![>]) {
					break
				}
				let punct = input.parse()?;
				params.push_punct(punct);
			}

			let gt_token: Token![>] = input.parse()?;

			Ok(Self {
				lt_token: Some(lt_token),
				params,
				gt_token: Some(gt_token),
				where_clause: None,
			})
		}
	}

	impl Parse for DynWhereClause {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			// copied from <syn::generics::WhereClause as syn::Parse>::parse
			Ok(Self {
				where_token: input.parse()?,
				predicates: {
					let mut predicates = Punctuated::new();
					loop {
						if input.is_empty()
							|| input.peek(token::Brace) || input.peek(Token![,])
							|| input.peek(Token![;]) || input.peek(Token![:])
							&& !input.peek(Token![::]) || input.peek(Token![=])
						{
							break
						}
						let value = input.parse()?;
						predicates.push_value(value);
						if !input.peek(Token![,]) {
							break
						}
						let punct = input.parse()?;
						predicates.push_punct(punct);
					}
					predicates
				},
			})
		}
	}

	impl Parse for DynWherePredicate {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			Ok(if input.peek(Token![dyn]) {
				Self::Dyn(input.parse::<DynWherePredicateSupertrait>()?)
			} else {
				input.parse::<WherePredicate>()?.into()
			})
		}
	}

	impl Parse for DynWherePredicateSupertrait {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			Ok(Self {
				dyn_token: input.parse()?,
				bounded_ty: input.parse()?,
				colon_token: input.parse()?,
				bounds: {
					// copied from <syn::generics::WherePredicate as syn::Parse>::parse
					let mut bounds = Punctuated::new();
					loop {
						if input.is_empty()
							|| input.peek(token::Brace) || input.peek(Token![,])
							|| input.peek(Token![;]) || input.peek(Token![:])
							&& !input.peek(Token![::]) || input.peek(Token![=])
						{
							break
						}
						let value = input.parse()?;
						bounds.push_value(value);
						if !input.peek(Token![+]) {
							break
						}
						let punct = input.parse()?;
						bounds.push_punct(punct);
					}
					bounds
				},
			})
		}
	}
}

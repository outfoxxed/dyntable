use parse::DynTrait;
use proc_macro2::TokenStream;
use process::DynTable;
use quote::ToTokens;
use syn::parse_macro_input;

use crate::parse::AttrConf;

//mod lib_old;

#[proc_macro_attribute]
pub fn dyntable(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	use syn::Ident;

	let dyn_trait = parse_macro_input!(item as DynTrait);
	let attrconf = parse_macro_input!(attr as AttrConf);
	let dyntable = DynTable {
		vtable_ident: Ident::new(
			&format!("{}VTable", dyn_trait.ident),
			dyn_trait.ident.span(),
		),
		dyntrait: dyn_trait,
		conf: attrconf,
	};
	let r = (|| -> syn::Result<TokenStream> {
		let mut token_stream = TokenStream::new();

		dyntable.build_vtable()?.to_tokens(&mut token_stream);
		dyntable.impl_vtable(&mut token_stream);
		dyntable.impl_vtable_repr(&mut token_stream);
		dyntable
			.impl_subtable()?
			.into_iter()
			.for_each(|table| table.to_tokens(&mut token_stream));
		dyntable.impl_dyntable(&mut token_stream)?;
		if let Some(func) = dyntable.impl_dyn_drop() {
			func.to_tokens(&mut token_stream);
		}
		if let Some(droptable) = dyntable.impl_droptable() {
			droptable.to_tokens(&mut token_stream);
		}
		dyntable.impl_trait_for_dyn()?.to_tokens(&mut token_stream);
		dyntable
			.dyntrait
			.strip_dyntable()
			.to_tokens(&mut token_stream);

		Ok(token_stream)
	})();

	match r {
		Ok(r) => r.into(),
		Err(e) => e.into_compile_error().into(),
	}
}

mod process {
	use std::{
		collections::{HashMap, HashSet},
		fmt::Debug,
	};

	use proc_macro2::{Span, TokenStream};
	use quote::ToTokens;
	use syn::{
		punctuated::Punctuated,
		spanned::Spanned,
		token::Paren,
		Abi,
		AngleBracketedGenericArguments,
		AttrStyle,
		Attribute,
		BareFnArg,
		Binding,
		Block,
		ConstParam,
		Expr,
		ExprCall,
		ExprCast,
		ExprField,
		ExprLet,
		ExprMethodCall,
		ExprParen,
		ExprPath,
		ExprReference,
		ExprStruct,
		ExprUnary,
		ExprUnsafe,
		Field,
		FieldValue,
		Fields,
		FieldsNamed,
		FnArg,
		GenericArgument,
		GenericParam,
		Generics,
		Ident,
		ImplItem,
		ImplItemConst,
		ImplItemMethod,
		ImplItemType,
		ItemFn,
		ItemImpl,
		ItemStruct,
		ItemTrait,
		Lifetime,
		LifetimeDef,
		LitStr,
		Member,
		Pat,
		PatIdent,
		PatPath,
		PatType,
		PatWild,
		Path,
		PathArguments,
		PathSegment,
		PredicateType,
		QSelf,
		Receiver,
		ReturnType,
		Signature,
		Stmt,
		Token,
		TraitBound,
		TraitBoundModifier,
		TraitItem,
		TraitItemConst,
		Type,
		TypeBareFn,
		TypeInfer,
		TypeParam,
		TypeParamBound,
		TypeParen,
		TypePath,
		TypePtr,
		TypeReference,
		TypeTraitObject,
		TypeTuple,
		UnOp,
		VisPublic,
		Visibility,
		WhereClause,
		WherePredicate,
	};

	use crate::parse::{
		AttrConf,
		Drop,
		DynTrait,
		DynWhereClause,
		DynWherePredicate,
		DynWherePredicateSupertrait,
	};

	#[derive(Debug, Clone)]
	pub struct DynTable {
		pub dyntrait: DynTrait,
		pub conf: AttrConf,
		pub vtable_ident: Ident,
		//vtable_repr: Attribute,
	}

	#[derive(Debug)]
	pub struct VtableData {
		pub path: Path,
		pub vtable_path: Path,
		pub vtable_generics: Generics,
	}

	#[derive(Debug, Clone)]
	pub struct SupertableGraph {
		pub node: Path,
		pub parents: Vec<SupertableGraph>,
	}

	impl SupertableGraph {
		/// Adds this node and its children to a Vec<Path>
		fn add_children<'a>(&'a self, vec: &mut Vec<&'a Path>) {
			vec.push(&self.node);
			for parent in &self.parents {
				parent.add_children(vec);
			}
		}

		/// Creates an iterator over all child nodes
		pub fn iter<'a>(&self) -> impl Iterator<Item = &Path> {
			let mut vec = Vec::<&Path>::new();

			for parent in &self.parents {
				parent.add_children(&mut vec);
			}

			vec.into_iter()
		}
	}

	macro_rules! path_arguments {
		(<[]>) => {
			PathArguments::None
		};
		(<[$($arg:expr),+]>) => {
			path_arguments!(
				[$($arg),+]
					.into_iter()
					.collect::<Punctuated<GenericArgument, _>>()
			)
		};
		($args:expr) => {
			PathArguments::AngleBracketed(AngleBracketedGenericArguments {
				colon2_token: Some(Default::default()),
				lt_token: Default::default(),
				gt_token: Default::default(),
				args: $args,
			})
		};
	}

	macro_rules! path_segments {
		($($segment:ident $([$($arg:expr),+])?)::+) => {
			[$(PathSegment {
				ident: Ident::new(stringify!($segment), Span::call_site()),
				arguments: path_arguments!(<[$($($arg),+)?]>),
			}),+]
				.into_iter()
				.collect::<Punctuated<_, _>>()
		}
	}

	macro_rules! path {
		($($segment:ident $([$($arg:expr),+])?)::+) => {
			Path {
				leading_colon: None,
				segments: path_segments!($($segment $([$($arg),+])?)::+),
			}
		};
		(::$($segment:ident $([$($arg:expr),+])?)::+) => {
			Path {
				leading_colon: Some(Default::default()),
				segments: path_segments!($($segment $([$($arg),+])?)::+),
			}
		};
	}

	/// extracts a graph of supertable inheritance
	pub fn extract_supertables(
		supertraits: impl IntoIterator<Item = TypeParamBound>,
		where_predicates: impl IntoIterator<Item = DynWherePredicate>,
	) -> syn::Result<Vec<SupertableGraph>> {
		let mut supertable_map = HashMap::<Path, Option<Punctuated<Path, Token![+]>>>::new();
		let mut specified_paths = HashSet::<Path>::new();

		// populate supertable map with supertable entries
		// from the where predicate iterator
		for entry in where_predicates.into_iter() {
			if let DynWherePredicate::Dyn(DynWherePredicateSupertrait {
				bounded_ty, bounds, ..
			}) = entry
			{
				match supertable_map.get(&bounded_ty) {
					None => {
						for bound in bounds.clone() {
							if specified_paths.contains(&bound) {
								return Err(syn::Error::new(
									bound.span(),
									"exactly one path to an inherited trait bound must be specified (this is a duplicate)",
								))
							} else {
								specified_paths.insert(bound);
							}
						}

						supertable_map.insert(bounded_ty, Some(bounds));
					},
					Some(_) => {
						return Err(syn::Error::new(
							bounded_ty.span(),
							"supertable already defined",
						))
					},
				}
			}
		}

		/// recursively graph a dyntable's supertables
		fn graph_supertables(
			path: Path,
			supertable_map: &HashMap<Path, Option<Punctuated<Path, syn::token::Add>>>,
			used_supertables: &mut HashSet<Path>,
		) -> SupertableGraph {
			let mut parents = Vec::<SupertableGraph>::new();

			if let Some(Some(supertables)) = supertable_map.get(&path) {
				for supertable in supertables {
					parents.push(graph_supertables(
						supertable.clone(),
						supertable_map,
						used_supertables,
					));
				}
			}

			let _ = used_supertables.insert(path.clone());

			SupertableGraph {
				node: path,
				parents,
			}
		}

		// populate the supertable graph starting from the trait's supertraits
		let mut supertables = Vec::<SupertableGraph>::new();
		let mut used_supertables = HashSet::<Path>::new();

		for supertrait in supertraits {
			if let TypeParamBound::Trait(TraitBound { path, .. }) = supertrait {
				if supertable_map.contains_key(&path) {
					supertables.push(graph_supertables(
						path,
						&supertable_map,
						&mut used_supertables,
					));
				}
			}
		}

		// check for any dangling dyn entries in the where predicate
		for supertable in supertable_map.keys() {
			if !used_supertables.contains(supertable) {
				return Err(syn::Error::new(
					supertable.span(),
					"dyn bound expression has no relation to the defined trait. did you mean to add a bound to the trait or a different supertrait?",
				))
			}
		}

		Ok(supertables)
	}

	/// Gets the path to a dyntable trait's vtable given a path to said trait.
	fn vtable_path(path: Path) -> Type {
		Type::Path(TypePath {
			qself: Some(QSelf {
				lt_token: Default::default(),
				ty: Box::new(Type::Paren(TypeParen {
					paren_token: Default::default(),
					elem: Box::new(Type::TraitObject(TypeTraitObject {
						dyn_token: Some(Default::default()),
						bounds: [
							TypeParamBound::Trait(TraitBound {
								paren_token: None,
								modifier: TraitBoundModifier::None,
								lifetimes: None,
								path,
							}),
							TypeParamBound::Lifetime(Lifetime::new("'static", Span::call_site())),
						]
						.into_iter()
						.collect(),
					})),
				})),
				position: 2,
				as_token: Default::default(),
				gt_token: Default::default(),
			}),
			path: path!(::dyntable::VTableRepr::VTable),
		})
	}

	fn generic_params_into_args(
		generics: impl IntoIterator<Item = GenericParam>,
	) -> impl Iterator<Item = GenericArgument> {
		generics.into_iter().map(|param| match param {
			GenericParam::Lifetime(lt) => GenericArgument::Lifetime(lt.lifetime),
			GenericParam::Type(TypeParam { ident, .. })
			| GenericParam::Const(ConstParam { ident, .. }) => GenericArgument::Type(Type::Path(TypePath {
				qself: None,
				path: Path {
					leading_colon: None,
					segments: [PathSegment::from(ident)].into_iter().collect(),
				},
			})),
		})
	}

	impl DynTable {
		fn abi(&self) -> Abi {
			Abi {
				extern_token: Default::default(),
				name: Some(LitStr::new(
					&self.conf.abi.to_string(),
					self.conf.abi.span(),
				)),
			}
		}

		fn drop_abi(&self) -> Option<Abi> {
			match &self.conf.drop {
				Drop::None => None,
				Drop::FollowAbi => Some(self.abi()),
				Drop::Abi(name) => Some(Abi {
					extern_token: Default::default(),
					name: Some(LitStr::new(&name.to_string(), name.span())),
				}),
			}
		}

		pub fn build_vtable(&self) -> syn::Result<ItemStruct> {
			let vtable_generics = self.dyntrait.generics.clone().strip_dyntable();

			let mut attributes = vec![Attribute {
				pound_token: Default::default(),
				style: AttrStyle::Outer,
				bracket_token: Default::default(),
				path: path!(allow),
				tokens: {
					let mut tokens = TokenStream::new();

					Paren {
						span: Span::call_site(),
					}
					.surround(&mut tokens, |tokens| {
						path!(non_snake_case).to_tokens(tokens);
					});

					tokens
				},
			}];

			if let Some(repr) = self.conf.repr.clone() {
				attributes.push(Attribute {
					pound_token: Default::default(),
					style: AttrStyle::Outer,
					bracket_token: Default::default(),
					path: path!(repr),
					tokens: {
						let mut tokens = TokenStream::new();

						Paren {
							span: Span::call_site(),
						}
						.surround(&mut tokens, |tokens| {
							Path::from(repr).to_tokens(tokens);
						});

						tokens
					},
				});
			}

			Ok(ItemStruct {
				attrs: attributes,
				vis: self.dyntrait.vis.clone(),
				struct_token: Default::default(),
				ident: self.vtable_ident.clone(),
				semi_token: None,
				fields: {
					let supertables = match &self.dyntrait.generics.where_clause {
						None => Vec::new(),
						Some(DynWhereClause { predicates, .. }) => extract_supertables(
							self.dyntrait.supertraits.clone(),
							predicates.clone(),
						)?,
					};

					let mut fields = Punctuated::<Field, _>::new();

					for SupertableGraph { node, .. } in supertables {
						fields.push(Field {
							attrs: Vec::new(),
							vis: Visibility::Public(VisPublic {
								pub_token: Default::default(),
							}),
							ident: Some(Ident::new(
								&format!("__vtable_{}", node.segments.last().unwrap().ident),
								node.segments.last().unwrap().span(),
							)),
							colon_token: Default::default(),
							ty: vtable_path(node),
						});
					}

					for item in &self.dyntrait.items {
						match item {
							TraitItem::Const(_) => {
								return Err(syn::Error::new(
									item.span(),
									"constants are not supported in dyntable traits",
								))
							},
							TraitItem::Type(_) => {
								return Err(syn::Error::new(
									item.span(),
									"associated types are not supported in dyntable traits",
								))
							},
							TraitItem::Macro(_) => {
								return Err(syn::Error::new(
									item.span(),
									"macro invocations are not supported in dyntable traits",
								))
							},
							TraitItem::Verbatim(_) => {
								return Err(syn::Error::new(item.span(), "unknown tokens"))
							},
							TraitItem::Method(method) => {
								let Signature {
									asyncness,
									unsafety,
									abi,
									fn_token,
									ident,
									generics,
									paren_token,
									inputs,
									variadic,
									output,
									..
								} = method.sig.clone();

								if asyncness.is_some() {
									return Err(syn::Error::new(
										asyncness.span(),
										"async methods are not supported in dyntable traits",
									))
								}

								let abi = match abi {
									Some(abi) => Some(abi),
									None => match &self.conf.abi.to_string() as &str {
										"Rust" => None,
										abi => {
											return Err(syn::Error::new(
												fn_token.span(),
												&format!(
													r#"missing `extern "ABI"` specifier (probably "{}")"#,
													abi
												),
											))
										},
									},
								};

								for param in &generics.params {
									match param {
										GenericParam::Const(_) => {
											return Err(syn::Error::new(
												param.span(),
												"const generics are not supported on methods in dyntable traits",
											))
										},
										GenericParam::Type(_) => {
											return Err(syn::Error::new(
												param.span(),
												"type generics are not supported on methods in dyntable traits",
											))
										},
										GenericParam::Lifetime(_) => {}, // implementations must uphold the lifetimes, so using raw pointers will be fine
									}
								}

								fields.push(Field {
									attrs: Vec::new(),
									vis: Visibility::Public(VisPublic {
										pub_token: Default::default(),
									}),
									ident: Some(ident),
									colon_token: Default::default(),
									ty: Type::BareFn(TypeBareFn {
										lifetimes: None,
										unsafety,
										abi,
										fn_token,
										paren_token,
										inputs: inputs
											.into_iter()
											.map(|input| match input {
												FnArg::Receiver(receiver) => {
													if receiver.reference.is_none() {
														return Err(syn::Error::new(
															receiver.span(),
															"methods are not allowed to take self by value in dyntable traits",
														));
													}

													Ok(BareFnArg {
														attrs: Vec::new(),
														name: None,
														ty: Type::Ptr(TypePtr {
															star_token: Default::default(),
															const_token: match &receiver.mutability
															{
																Some(_) => None,
																None => Default::default(),
															},
															mutability: receiver.mutability,
															elem: Box::new(Type::Path(TypePath {
																qself: None,
																path: path!(::core::ffi::c_void),
															})),
														}),
													})
												},
												FnArg::Typed(arg) => Ok(BareFnArg {
													attrs: Vec::new(),
													name: None,
													ty: match *arg.ty {
														Type::Reference(reference) => {
															Type::Ptr(TypePtr {
																star_token: Default::default(),
																const_token: match &reference
																	.mutability
																{
																	Some(_) => None,
																	None => Default::default(),
																},
																mutability: reference.mutability,
																elem: reference.elem,
															})
														},
														ty => ty,
													},
												}),
											})
											.collect::<Result<_, _>>()?,
										variadic,
										output: match output {
											ReturnType::Type(arrow, ty) => ReturnType::Type(
												arrow,
												Box::new(match *ty {
													Type::Reference(reference) => {
														Type::Ptr(TypePtr {
															star_token: Default::default(),
															const_token: match &reference.mutability
															{
																Some(_) => None,
																None => Default::default(),
															},
															mutability: reference.mutability,
															elem: reference.elem,
														})
													},
													ty => ty,
												}),
											),
											output => output,
										},
									}),
								})
							},
							_ => return Err(syn::Error::new(item.span(), "unknown trait item")),
						}
					}

					if let Some(drop_abi) = self.drop_abi() {
						fields.push(Field {
							attrs: Vec::new(),
							vis: Visibility::Inherited,
							ident: Some(Ident::new("__drop", Span::call_site())),
							colon_token: Default::default(),
							ty: Type::BareFn(TypeBareFn {
								lifetimes: None,
								unsafety: Some(Default::default()),
								abi: Some(drop_abi),
								fn_token: Default::default(),
								paren_token: Default::default(),
								inputs: [BareFnArg {
									attrs: Vec::new(),
									name: None,
									ty: Type::Ptr(TypePtr {
										star_token: Default::default(),
										const_token: None,
										mutability: Some(Default::default()),
										elem: Box::new(Type::Path(TypePath {
											qself: None,
											path: path!(::core::ffi::c_void),
										})),
									}),
								}]
								.into_iter()
								.collect(),
								variadic: None,
								output: ReturnType::Default,
							}),
						});
					}

					fields.push(Field {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						ident: Some(Ident::new("__generics", Span::call_site())),
						colon_token: Default::default(),
						ty: Type::Path(TypePath {
							qself: None,
							path: Path {
								leading_colon: Some(Default::default()),
								segments: {
									let mut phantom_types = vtable_generics
										.params
										.iter()
										.filter_map(|param| match param {
											GenericParam::Type(ty) => Some(Type::Path(TypePath {
												qself: None,
												path: Path {
													leading_colon: None,
													segments: [PathSegment::from(ty.ident.clone())]
														.into_iter()
														.collect(),
												},
											})),
											GenericParam::Lifetime(lt) => {
												Some(Type::Reference(TypeReference {
													and_token: Default::default(),
													lifetime: Some(lt.lifetime.clone()),
													mutability: None,
													elem: Box::new(Type::Tuple(TypeTuple {
														paren_token: Default::default(),
														elems: Punctuated::new(),
													})),
												}))
											},
											GenericParam::Const(_) => None,
										})
										.collect::<Vec<_>>();

									let phantom_type = match phantom_types.len() {
										0 => Type::Tuple(TypeTuple {
											paren_token: Default::default(),
											elems: Punctuated::new(),
										}),
										1 => phantom_types.remove(0),
										_ => Type::Tuple(TypeTuple {
											paren_token: Default::default(),
											elems: phantom_types.into_iter().collect(),
										}),
									};

									let mut punctuated = path_segments!(core::marker::PhantomData);

									let last = punctuated.last_mut().unwrap();
									last.arguments =
										path_arguments!(<[GenericArgument::Type(phantom_type)]>);

									punctuated
								},
							},
						}),
					});

					Fields::Named(FieldsNamed {
						brace_token: Default::default(),
						named: fields,
					})
				},
				generics: vtable_generics,
			})
		}

		pub fn impl_vtable(&self, tokens: &mut TokenStream) {
			let vtable_generics = self.dyntrait.generics.clone().strip_dyntable();

			let bounds = if self.dyntrait.supertraits.is_empty() {
				Punctuated::from_iter([TypeParamBound::Trait(TraitBound {
					paren_token: None,
					modifier: TraitBoundModifier::None,
					lifetimes: None,
					path: path!(::dyntable::__private::NoBounds),
				})])
			} else {
				let trait_bounds = self
					.dyntrait
					.supertraits
					.clone()
					.into_iter()
					.filter_map(|param| match param {
						TypeParamBound::Trait(TraitBound { path, .. }) => {
							Some(TypeParamBound::Trait(TraitBound {
								paren_token: None,
								modifier: TraitBoundModifier::None,
								lifetimes: None,
								path,
							}))
						},
						_ => None,
					})
					.collect::<Punctuated<_, _>>();

				if trait_bounds.len() == 1 {
					trait_bounds
				} else {
					let type_ident = Ident::new(
						&format!("__DynBounds_{}", self.dyntrait.ident.to_string()),
						Span::call_site(),
					);

					ItemTrait {
						attrs: vec![Attribute {
							pound_token: Default::default(),
							style: AttrStyle::Outer,
							bracket_token: Default::default(),
							path: path!(allow),
							tokens: {
								let mut tokens = TokenStream::new();

								Paren {
									span: Span::call_site(),
								}
								.surround(&mut tokens, |tokens| {
									path!(non_camel_case_types).to_tokens(tokens);
								});

								tokens
							},
						}],
						vis: Visibility::Inherited,
						unsafety: None,
						auto_token: None,
						trait_token: Default::default(),
						ident: type_ident.clone(),
						generics: self.dyntrait.generics.clone().strip_dyntable(),
						colon_token: Some(Default::default()),
						supertraits: self.dyntrait.supertraits.clone(),
						brace_token: Default::default(),
						items: Vec::new(),
					}
					.to_tokens(tokens);

					Punctuated::from_iter([TypeParamBound::Trait(TraitBound {
						paren_token: None,
						modifier: TraitBoundModifier::None,
						lifetimes: None,
						path: Path {
							leading_colon: None,
							segments: [PathSegment {
								ident: type_ident,
								arguments: PathArguments::AngleBracketed(
									AngleBracketedGenericArguments {
										colon2_token: None,
										lt_token: Default::default(),
										gt_token: Default::default(),
										args: generic_params_into_args(
											self.dyntrait.generics.params.clone(),
										)
										.collect(),
									},
								),
							}]
							.into_iter()
							.collect(),
						},
					})])
				}
			};

			ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: Some(Default::default()),
				impl_token: Default::default(),
				trait_: Some((None, path!(::dyntable::VTable), <Token![for]>::default())),
				self_ty: Box::new(Type::Path(TypePath {
					qself: None,
					path: Path::from(PathSegment {
						ident: self.vtable_ident.clone(),
						arguments: path_arguments!(generic_params_into_args(
							vtable_generics.params.clone()
						)
						.collect()),
					}),
				})),
				generics: vtable_generics,
				brace_token: Default::default(),
				items: vec![ImplItem::Type(ImplItemType {
					attrs: Vec::new(),
					vis: Visibility::Inherited,
					defaultness: None,
					type_token: Default::default(),
					ident: Ident::new("Bounds", Span::call_site()),
					generics: Generics {
						lt_token: None,
						gt_token: None,
						params: Punctuated::new(),
						where_clause: None,
					},
					eq_token: Default::default(),
					semi_token: Default::default(),
					ty: Type::TraitObject(TypeTraitObject {
						dyn_token: Some(Default::default()),
						bounds,
					}),
				})],
			}
			.to_tokens(tokens);
		}

		pub fn impl_vtable_repr(&self, tokens: &mut TokenStream) {
			let vtable_type = Type::Path(TypePath {
				qself: None,
				path: Path::from(PathSegment {
					ident: self.vtable_ident.clone(),
					arguments: path_arguments!(generic_params_into_args(
						self.dyntrait.generics.clone().strip_dyntable().params
					)
					.collect()),
				}),
			});

			let mut make_impl = |additional_bounds: &[Path], vtable_wrapper: Option<Path>| {
				ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: None,
				impl_token: Default::default(),
				generics: self.dyntrait.generics.clone().strip_dyntable(),
				trait_: Some((
					None,
					path!(::dyntable::VTableRepr),
					<Token![for]>::default(),
				)),
				self_ty: Box::new(Type::TraitObject(TypeTraitObject {
					dyn_token: Some(Default::default()),
					bounds: [TypeParamBound::Trait(TraitBound {
						paren_token: None,
						modifier: TraitBoundModifier::None,
						lifetimes: None,
						path: Path::from(PathSegment {
							ident: self.dyntrait.ident.clone(),
							arguments: path_arguments!(generic_params_into_args(
								self.dyntrait.generics.clone().strip_dyntable().params
							)
							.collect()),
						}),
					})]
						.into_iter()
						.chain(
							additional_bounds.into_iter()
								.map(|path| TypeParamBound::Trait(TraitBound {
									paren_token: None,
									modifier: TraitBoundModifier::None,
									lifetimes: None,
									path: path.clone(),
								}))
						)
						.collect(),
				})),
				brace_token: Default::default(),
				items: vec![ImplItem::Type(ImplItemType {
					attrs: Vec::new(),
					vis: Visibility::Inherited,
					defaultness: None,
					type_token: Default::default(),
					ident: Ident::new("VTable", Span::call_site()),
					generics: Generics {
						lt_token: None,
						params: Punctuated::new(),
						gt_token: None,
						where_clause: None,
					},
					eq_token: Default::default(),
					ty: match vtable_wrapper {
						Some(mut wrapper) => Type::Path(TypePath {
							qself: None,
							path: {
								wrapper.segments.last_mut().unwrap().arguments = path_arguments!(<[GenericArgument::Type(vtable_type.clone())]>);
								wrapper
							},
						}),
						None => vtable_type.clone(),
					},
					semi_token: Default::default(),
				})],
			}.to_tokens(tokens)
			};

			make_impl(&[], None);
			make_impl(
				&[path!(::core::marker::Send)],
				Some(path!(::dyntable::__private::SendVTable)),
			);
			make_impl(
				&[path!(::core::marker::Sync)],
				Some(path!(::dyntable::__private::SyncVTable)),
			);
			make_impl(
				&[path!(::core::marker::Send), path!(::core::marker::Sync)],
				Some(path!(::dyntable::__private::SendSyncVTable)),
			);
		}

		pub fn impl_subtable(&self) -> syn::Result<Vec<ItemImpl>> {
			fn impl_subtable(
				generics: Generics,
				supertable: Path,
				vtable: Ident,
				block: Block,
			) -> ItemImpl {
				let vtable_path = vtable_path(supertable);
				ItemImpl {
					attrs: Vec::new(),
					defaultness: None,
					unsafety: None,
					impl_token: Default::default(),
					trait_: Some((
						None,
						path!(::dyntable::SubTable[GenericArgument::Type(vtable_path.clone())]),
						<Token![for]>::default(),
					)),
					self_ty: Box::new(Type::Path(TypePath {
						qself: None,
						path: Path::from(PathSegment {
							ident: vtable,
							arguments: path_arguments!(generic_params_into_args(
								generics.params.clone()
							)
							.collect()),
						}),
					})),
					generics,
					brace_token: Default::default(),
					items: vec![ImplItem::Method(ImplItemMethod {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						defaultness: None,
						sig: Signature {
							constness: None,
							asyncness: None,
							unsafety: None,
							abi: None,
							fn_token: Default::default(),
							ident: Ident::new("subtable", Span::call_site()),
							generics: Generics {
								lt_token: None,
								params: Punctuated::new(),
								gt_token: None,
								where_clause: None,
							},
							paren_token: Default::default(),
							inputs: [FnArg::Receiver(Receiver {
								attrs: Vec::new(),
								reference: Some((<Token![&]>::default(), None)),
								mutability: None,
								self_token: Default::default(),
							})]
							.into_iter()
							.collect(),
							variadic: None,
							output: ReturnType::Type(
								Default::default(),
								Box::new(Type::Reference(TypeReference {
									and_token: Default::default(),
									lifetime: None,
									mutability: None,
									elem: Box::new(vtable_path),
								})),
							),
						},
						block,
					})],
				}
			}

			let mut impls = Vec::<ItemImpl>::new();

			if let Some(where_clause) = &self.dyntrait.generics.where_clause {
				let supertables = extract_supertables(
					self.dyntrait.supertraits.clone(),
					where_clause.predicates.clone(),
				)?;

				let vtable_generics = self.dyntrait.generics.clone().strip_dyntable();

				for supertable in supertables {
					impls.push(impl_subtable(
						vtable_generics.clone(),
						supertable.node.clone(),
						self.vtable_ident.clone(),
						Block {
							brace_token: Default::default(),
							stmts: vec![Stmt::Expr(Expr::Reference(ExprReference {
								attrs: Vec::new(),
								and_token: Default::default(),
								raw: Default::default(),
								mutability: None,
								expr: Box::new(Expr::Field(ExprField {
									attrs: Vec::new(),
									base: Box::new(Expr::Path(ExprPath {
										attrs: Vec::new(),
										qself: None,
										path: path!(self),
									})),
									dot_token: Default::default(),
									member: Member::Named(Ident::new(
										&format!(
											"__vtable_{}",
											supertable.node.segments.last().unwrap().ident
										),
										Span::call_site(),
									)),
								})),
							}))],
						},
					));

					// indirect supertables
					for indirect in supertable.iter() {
						impls.push(impl_subtable(
							vtable_generics.clone(),
							indirect.clone(),
							self.vtable_ident.clone(),
							Block {
								brace_token: Default::default(),
								stmts: vec![Stmt::Expr(Expr::MethodCall(ExprMethodCall {
									attrs: Vec::new(),
									receiver: Box::new(Expr::Call(ExprCall {
										attrs: Vec::new(),
										func: Box::new(Expr::Path(ExprPath {
											attrs: Vec::new(),
											qself: None,
											path: path!(
												::dyntable::SubTable[GenericArgument::Type(vtable_path(supertable.node.clone()))]::subtable
											),
										})),
										paren_token: Default::default(),
										args: [Expr::Path(ExprPath {
											attrs: Vec::new(),
											qself: None,
											path: path!(self),
										})]
										.into_iter()
										.collect(),
									})),
									dot_token: Default::default(),
									method: Ident::new("subtable", Span::call_site()),
									turbofish: None,
									paren_token: Default::default(),
									args: Punctuated::new(),
								}))],
							},
						));
					}
				}
			}

			Ok(impls)
		}

		pub fn impl_dyntable(&self, tokens: &mut TokenStream) -> syn::Result<()> {
			let vtable_generics = self.dyntrait.generics.clone().strip_dyntable();

			let dyntable_proxy_ident = Ident::new(
				&format!("__DynTable_{}", self.dyntrait.ident.to_string()),
				Span::call_site(),
			);
			let vtable_lifetime = Lifetime::new("'__dyn_vtable", Span::call_site());
			let impl_target_ident = Ident::new("__DynTarget", Span::call_site());
			let vtable_type = Type::Path(TypePath {
				qself: None,
				path: Path::from(PathSegment {
					ident: self.vtable_ident.clone(),
					arguments: path_arguments!(
						generic_params_into_args(vtable_generics.params).collect()
					),
				}),
			});

			// DynTable proxy trait
			ItemTrait {
				attrs: vec![Attribute {
					pound_token: Default::default(),
					style: AttrStyle::Outer,
					bracket_token: Default::default(),
					path: path!(allow),
					tokens: {
						let mut tokens = TokenStream::new();

						Paren {
							span: Span::call_site(),
						}
						.surround(&mut tokens, |tokens| {
							path!(non_camel_case_types).to_tokens(tokens)
						});

						tokens
					},
				}],
				vis: Visibility::Inherited,
				unsafety: Some(Default::default()),
				auto_token: None,
				trait_token: Default::default(),
				ident: dyntable_proxy_ident.clone(),
				generics: Generics {
					lt_token: Some(Default::default()),
					gt_token: Some(Default::default()),
					params: [
						GenericParam::Lifetime(LifetimeDef {
							attrs: Vec::new(),
							lifetime: vtable_lifetime.clone(),
							colon_token: None,
							bounds: Punctuated::new(),
						}),
						GenericParam::Type(TypeParam {
							attrs: Vec::new(),
							ident: Ident::new("V", Span::call_site()),
							colon_token: Some(Default::default()),
							bounds: [
								TypeParamBound::Lifetime(vtable_lifetime.clone()),
								TypeParamBound::Trait(TraitBound {
									paren_token: None,
									modifier: TraitBoundModifier::None,
									lifetimes: None,
									path: path!(::dyntable::VTable),
								}),
							]
							.into_iter()
							.collect(),
							eq_token: None,
							default: None,
						}),
					]
					.into_iter()
					.collect(),
					where_clause: None,
				},
				colon_token: Default::default(),
				supertraits: Punctuated::new(),
				brace_token: Default::default(),
				items: vec![
					TraitItem::Const(TraitItemConst {
						attrs: Vec::new(),
						const_token: Default::default(),
						ident: Ident::new("VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: Type::Path(TypePath {
							qself: None,
							path: path!(V),
						}),
						default: None,
						semi_token: Default::default(),
					}),
					TraitItem::Const(TraitItemConst {
						attrs: Vec::new(),
						const_token: Default::default(),
						ident: Ident::new("STATIC_VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: Type::Reference(TypeReference {
							and_token: Default::default(),
							lifetime: Some(vtable_lifetime.clone()),
							mutability: None,
							elem: Box::new(Type::Path(TypePath {
								qself: None,
								path: path!(V),
							})),
						}),
						default: None,
						semi_token: Default::default(),
					}),
				],
			}
			.to_tokens(tokens);

			// Impl for DynImplTarget
			ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: Some(Default::default()),
				impl_token: Default::default(),
				generics: {
					let mut generics = self.dyntrait.generics.clone().strip_dyntable();

					for param in &mut generics.params {
						match param {
							GenericParam::Lifetime(param) => {
								param.bounds.insert(0, vtable_lifetime.clone());
							},
							GenericParam::Type(param) => {
								param
									.bounds
									.insert(0, TypeParamBound::Lifetime(vtable_lifetime.clone()));
							},
							GenericParam::Const(_) => {},
						}
					}

					generics.params.insert(
						0,
						GenericParam::Lifetime(LifetimeDef {
							attrs: Vec::new(),
							lifetime: vtable_lifetime.clone(),
							colon_token: None,
							bounds: Punctuated::new(),
						}),
					);

					generics.params.push(GenericParam::Type(TypeParam {
						attrs: Vec::new(),
						ident: Ident::new("__DynTable", Span::call_site()),
						colon_token: None,
						bounds: Punctuated::new(),
						eq_token: None,
						default: None,
					}));

					let where_clause = generics.where_clause.get_or_insert_with(|| WhereClause {
						where_token: Default::default(),
						predicates: Punctuated::new(),
					});

					where_clause
						.predicates
						.push(WherePredicate::Type(PredicateType {
							lifetimes: None,
							bounded_ty: Type::Path(TypePath {
								qself: None,
								path: path!(__DynTable),
							}),
							colon_token: Default::default(),
							bounds: [TypeParamBound::Trait(TraitBound {
								paren_token: Some(Default::default()),
								modifier: TraitBoundModifier::None,
								lifetimes: None,
								path: Path {
									leading_colon: None,
									segments: [PathSegment {
										ident: dyntable_proxy_ident.clone(),
										arguments: path_arguments!(<[
												GenericArgument::Lifetime(vtable_lifetime.clone()),
												GenericArgument::Type(vtable_type.clone())
											]>),
									}]
									.into_iter()
									.collect(),
								},
							})]
							.into_iter()
							.collect(),
						}));

					generics
				},
				trait_: Some((
					None,
					path!(
						::dyntable::__private::DynTable2[
							GenericArgument::Lifetime(vtable_lifetime.clone()),
							GenericArgument::Type(vtable_type.clone())
						]
					),
					<Token![for]>::default(),
				)),
				self_ty: Box::new(Type::Path(TypePath {
					qself: None,
					path: path!(::dyntable::__private::DynImplTarget[
						GenericArgument::Type(Type::Path(TypePath {
							qself: None,
							path: path!(__DynTable),
						})),
						GenericArgument::Type(vtable_type.clone())
					]),
				})),
				brace_token: Default::default(),
				items: vec![
					ImplItem::Const(ImplItemConst {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						defaultness: None,
						const_token: Default::default(),
						ident: Ident::new("VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: vtable_type.clone(),
						eq_token: Default::default(),
						semi_token: Default::default(),
						expr: Expr::Path(ExprPath {
							attrs: Vec::new(),
							qself: None,
							path: path!(__DynTable::VTABLE),
						}),
					}),
					ImplItem::Const(ImplItemConst {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						defaultness: None,
						const_token: Default::default(),
						ident: Ident::new("STATIC_VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: Type::Reference(TypeReference {
							and_token: Default::default(),
							lifetime: Some(vtable_lifetime.clone()),
							mutability: None,
							elem: Box::new(vtable_type.clone()),
						}),
						eq_token: Default::default(),
						semi_token: Default::default(),
						expr: Expr::Path(ExprPath {
							attrs: Vec::new(),
							qself: None,
							path: path!(__DynTable::STATIC_VTABLE),
						}),
					}),
				],
			}
			.to_tokens(tokens);

			// DynTable (proxy) impl
			ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: Some(Default::default()),
				impl_token: Default::default(),
				generics: {
					let mut generics = self.dyntrait.generics.clone().strip_dyntable();

					for param in &mut generics.params {
						match param {
							GenericParam::Lifetime(param) => {
								param.bounds.insert(0, vtable_lifetime.clone());
							},
							GenericParam::Type(param) => {
								param
									.bounds
									.insert(0, TypeParamBound::Lifetime(vtable_lifetime.clone()));
							},
							GenericParam::Const(_) => {},
						}
					}

					generics.params.insert(
						0,
						GenericParam::Lifetime(LifetimeDef {
							attrs: Vec::new(),
							lifetime: vtable_lifetime.clone(),
							colon_token: None,
							bounds: Punctuated::new(),
						}),
					);

					generics.params.push(GenericParam::Type(TypeParam {
						attrs: Vec::new(),
						ident: impl_target_ident.clone(),
						colon_token: Some(Default::default()),
						bounds: [TypeParamBound::Trait(TraitBound {
							paren_token: None,
							modifier: TraitBoundModifier::None,
							lifetimes: None,
							path: Path::from(PathSegment {
								ident: self.dyntrait.ident.clone(),
								arguments: path_arguments!(generic_params_into_args(
									self.dyntrait.generics.params.clone()
								)
								.collect()),
							}),
						})]
						.into_iter()
						.collect(),
						eq_token: None,
						default: None,
					}));

					generics
				},
				trait_: Some((
					None,
					Path {
						leading_colon: None,
						segments: [PathSegment {
							ident: dyntable_proxy_ident.clone(),
							arguments: path_arguments!(<[
								GenericArgument::Lifetime(vtable_lifetime.clone()),
								GenericArgument::Type(vtable_type.clone())
							]>),
						}]
						.into_iter()
						.collect(),
					},
					<Token![for]>::default(),
				)),
				self_ty: Box::new(Type::Path(TypePath {
					qself: None,
					path: Path::from(impl_target_ident.clone()),
				})),
				brace_token: Default::default(),
				items: vec![
					ImplItem::Const(ImplItemConst {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						defaultness: None,
						const_token: Default::default(),
						ident: Ident::new("STATIC_VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: Type::Reference(TypeReference {
							and_token: Default::default(),
							lifetime: Some(vtable_lifetime.clone()),
							mutability: None,
							elem: Box::new(vtable_type.clone()),
						}),
						eq_token: Default::default(),
						semi_token: Default::default(),
						expr: Expr::Reference(ExprReference {
							attrs: Vec::new(),
							and_token: Default::default(),
							raw: Default::default(),
							mutability: None,
							expr: Box::new(Expr::Path(ExprPath {
								attrs: Vec::new(),
								qself: Some(QSelf {
									lt_token: Default::default(),
									gt_token: Default::default(),
									as_token: Some(Default::default()),
									ty: Box::new(Type::Path(TypePath {
										qself: None,
										path: path!(Self),
									})),
									position: 1,
								}),
								path: Path {
									leading_colon: None,
									segments: [
										PathSegment {
											ident: dyntable_proxy_ident.clone(),
											arguments: path_arguments!(<[GenericArgument::Type(vtable_type.clone())]>),
										},
										PathSegment::from(Ident::new("VTABLE", Span::call_site())),
									]
									.into_iter()
									.collect(),
								},
							})),
						}),
					}),
					ImplItem::Const(ImplItemConst {
						attrs: Vec::new(),
						vis: Visibility::Inherited,
						defaultness: None,
						const_token: Default::default(),
						ident: Ident::new("VTABLE", Span::call_site()),
						colon_token: Default::default(),
						ty: vtable_type,
						eq_token: Default::default(),
						semi_token: Default::default(),
						expr: Expr::Struct(ExprStruct {
							attrs: Vec::new(),
							path: Path::from(self.vtable_ident.clone()),
							brace_token: Default::default(),
							fields: {
								let mut fields = Punctuated::<FieldValue, _>::new();

								let supertables = match &self.dyntrait.generics.where_clause {
									None => Vec::new(),
									Some(DynWhereClause { predicates, .. }) => extract_supertables(
										self.dyntrait.supertraits.clone(),
										predicates.clone(),
									)?,
								};

								for SupertableGraph { node, .. } in supertables {
									fields.push(FieldValue {
										attrs: Vec::new(),
										member: Member::Named(Ident::new(
											&format!(
												"__vtable_{}",
												node.segments.last().unwrap().ident
											),
											node.segments.last().unwrap().span(),
										)),
										colon_token: Some(Default::default()),
										expr: Expr::Path(ExprPath {
											attrs: Vec::new(),
											qself: Some(QSelf {
												lt_token: Default::default(),
												gt_token: Default::default(),
												as_token: Default::default(),
												position: 2,
												ty: Box::new(Type::Path(TypePath {
													qself: None,
													path: Path::from(impl_target_ident.clone()),
												})),
											}),
											path: path!(
												::dyntable::DynTable[GenericArgument::Type(vtable_path(node))]::VTABLE
											),
										}),
									});
								}

								for item in &self.dyntrait.items {
									// item validation is already done in build_vtable
									if let TraitItem::Method(method) = item {
										let Signature {
											unsafety,
											abi,
											fn_token,
											ident,
											paren_token,
											inputs,
											variadic,
											output,
											..
										} = method.sig.clone();

										// checked in build_vtable
										let abi = abi.unwrap_or_else(|| self.abi());

										fields.push(FieldValue {
											attrs: Vec::new(),
											member: Member::Named(ident.clone()),
											colon_token: Some(Default::default()),
											expr: Expr::Unsafe(ExprUnsafe {
												attrs: Vec::new(),
												unsafe_token: Default::default(),
												block: Block {
													brace_token: Default::default(),
													stmts: vec![Stmt::Expr(Expr::Call(ExprCall {
														attrs: Vec::new(),
														func: Box::new(Expr::Path(ExprPath {
															attrs: Vec::new(),
															qself: None,
															path: path!(::core::intrinsics::transmute),
														})),
														paren_token: Default::default(),
														args: [Expr::Cast(ExprCast {
															attrs: Vec::new(),
															as_token: Default::default(),
															expr: Box::new(Expr::Path(ExprPath {
																attrs: Vec::new(),
																qself: None,
																path: Path {
																	leading_colon: None,
																	segments: [
																		impl_target_ident.clone(),
																		ident.clone(),
																	]
																	.map(|ident| PathSegment::from(ident))
																	.into_iter()
																	.collect(),
																},
															})),
															ty: Box::new(Type::BareFn(TypeBareFn {
																lifetimes: None,
																unsafety,
																abi: Some(abi),
																fn_token,
																paren_token,
																inputs: inputs
																	.into_iter()
																	.map(|_| BareFnArg {
																		attrs: Vec::new(),
																		name: None,
																		ty: Type::Infer(TypeInfer {
																			underscore_token: Default::default(),
																		}),
																	})
																	.collect(),
																variadic,
																output: match output {
																	ReturnType::Default => ReturnType::Default,
																	ReturnType::Type(arrow, _) => ReturnType::Type(
																		arrow,
																		Box::new(Type::Infer(TypeInfer {
																			underscore_token: Default::default(),
																		})),
																	),
																},
															})),
														})].into_iter().collect(),
													}))],
												},
											}),
										});
									}
								}

								match self.conf.drop {
									Drop::None => {},
									_ => {
										fields.push(FieldValue {
											attrs: Vec::new(),
											member: Member::Named(Ident::new(
												"__drop",
												Span::call_site(),
											)),
											colon_token: Some(Default::default()),
											expr: Expr::Path(ExprPath {
												attrs: Vec::new(),
												qself: None,
												path: Path {
													leading_colon: None,
													segments: [PathSegment {
														ident: Ident::new(
															&format!(
																"__{}_drop",
																self.dyntrait.ident
															),
															Span::call_site(),
														),
														arguments: path_arguments!(<[
															GenericArgument::Type(Type::Path(TypePath {
																qself: None,
																path: Path::from(impl_target_ident),
															}))
														]>),
													}]
													.into_iter()
													.collect(),
												},
											}),
										});
									},
								}

								fields.push(FieldValue {
									attrs: Vec::new(),
									member: Member::Named(Ident::new(
										"__generics",
										Span::call_site(),
									)),
									colon_token: Some(Default::default()),
									expr: Expr::Path(ExprPath {
										attrs: Vec::new(),
										qself: None,
										path: path!(::core::marker::PhantomData),
									}),
								});

								fields
							},
							dot2_token: None,
							rest: None,
						}),
					}),
				],
			}
			.to_tokens(tokens);

			Ok(())
		}

		pub fn impl_dyn_drop(&self) -> Option<ItemFn> {
			self.drop_abi().map(|drop_abi| ItemFn {
				attrs: vec![Attribute {
					pound_token: Default::default(),
					style: AttrStyle::Outer,
					bracket_token: Default::default(),
					path: path!(allow),
					tokens: {
						let mut tokens = TokenStream::new();

						Paren {
							span: Span::call_site(),
						}
						.surround(&mut tokens, |tokens| {
							path!(non_snake_case).to_tokens(tokens);
						});

						tokens
					},
				}],
				vis: Visibility::Inherited,
				sig: Signature {
					constness: None,
					asyncness: None,
					unsafety: Some(Default::default()),
					abi: Some(drop_abi),
					fn_token: Default::default(),
					ident: Ident::new(
						&format!("__{}_drop", self.dyntrait.ident),
						Span::call_site(),
					),
					generics: Generics {
						lt_token: Default::default(),
						gt_token: Default::default(),
						params: [GenericParam::Type(TypeParam {
							attrs: Vec::new(),
							ident: Ident::new("T", Span::call_site()),
							colon_token: None,
							bounds: Punctuated::new(),
							eq_token: None,
							default: None,
						})]
						.into_iter()
						.collect(),
						where_clause: None,
					},
					paren_token: Default::default(),
					inputs: [FnArg::Typed(PatType {
						attrs: Vec::new(),
						pat: Box::new(Pat::Ident(PatIdent {
							attrs: Vec::new(),
							by_ref: None,
							mutability: None,
							ident: Ident::new("ptr", Span::call_site()),
							subpat: None,
						})),
						colon_token: Default::default(),
						ty: Box::new(Type::Ptr(TypePtr {
							star_token: Default::default(),
							const_token: None,
							mutability: Some(Default::default()),
							elem: Box::new(Type::Path(TypePath {
								qself: None,
								path: path!(::core::ffi::c_void),
							})),
						})),
					})]
					.into_iter()
					.collect(),
					variadic: None,
					output: ReturnType::Default,
				},
				block: Box::new(Block {
					brace_token: Default::default(),
					stmts: vec![Stmt::Semi(
						Expr::Let(ExprLet {
							attrs: Vec::new(),
							let_token: Default::default(),
							pat: Pat::Wild(PatWild {
								attrs: Vec::new(),
								underscore_token: Default::default(),
							}),
							eq_token: Default::default(),
							expr: Box::new(Expr::Call(ExprCall {
								attrs: Vec::new(),
								func: Box::new(Expr::Path(ExprPath {
									attrs: Vec::new(),
									qself: None,
									path: path!(::std::boxed::Box::from_raw),
								})),
								paren_token: Default::default(),
								args: [Expr::Cast(ExprCast {
									attrs: Vec::new(),
									expr: Box::new(Expr::Path(ExprPath {
										attrs: Vec::new(),
										qself: None,
										path: path!(ptr),
									})),
									as_token: Default::default(),
									ty: Box::new(Type::Ptr(TypePtr {
										star_token: Default::default(),
										const_token: None,
										mutability: Some(Default::default()),
										elem: Box::new(Type::Path(TypePath {
											qself: None,
											path: path!(T),
										})),
									})),
								})]
								.into_iter()
								.collect(),
							})),
						}),
						<Token![;]>::default(),
					)],
				}),
			})
		}

		pub fn impl_droptable(&self) -> Option<ItemImpl> {
			if let Drop::None = self.conf.drop {
				return None
			}

			let vtable_generics = self.dyntrait.generics.clone().strip_dyntable();

			Some(ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: Some(Default::default()),
				impl_token: Default::default(),
				trait_: Some((None, path!(::dyntable::DropTable), <Token![for]>::default())),
				self_ty: Box::new(Type::Path(TypePath {
					qself: None,
					path: Path::from(PathSegment {
						ident: self.vtable_ident.clone(),
						arguments: path_arguments!(generic_params_into_args(
							vtable_generics.params.clone()
						)
						.collect()),
					}),
				})),
				generics: vtable_generics,
				brace_token: Default::default(),
				items: vec![ImplItem::Method(ImplItemMethod {
					attrs: Vec::new(),
					vis: Visibility::Inherited,
					defaultness: None,
					sig: Signature {
						constness: None,
						asyncness: None,
						unsafety: Some(Default::default()),
						abi: None,
						fn_token: Default::default(),
						ident: Ident::new("virtual_drop", Span::call_site()),
						generics: Generics {
							lt_token: None,
							gt_token: None,
							params: Punctuated::new(),
							where_clause: None,
						},
						paren_token: Default::default(),
						inputs: [
							FnArg::Receiver(Receiver {
								attrs: Vec::new(),
								reference: Some((<Token![&]>::default(), None)),
								mutability: None,
								self_token: Default::default(),
							}),
							FnArg::Typed(PatType {
								attrs: Vec::new(),
								pat: Box::new(Pat::Path(PatPath {
									attrs: Vec::new(),
									qself: None,
									path: path!(instance),
								})),
								colon_token: Default::default(),
								ty: Box::new(Type::Ptr(TypePtr {
									star_token: Default::default(),
									const_token: None,
									mutability: Some(Default::default()),
									elem: Box::new(Type::Path(TypePath {
										qself: None,
										path: path!(::core::ffi::c_void),
									})),
								})),
							}),
						]
						.into_iter()
						.collect(),
						variadic: None,
						output: ReturnType::Default,
					},
					block: Block {
						brace_token: Default::default(),
						stmts: vec![Stmt::Expr(Expr::Call(ExprCall {
							attrs: Vec::new(),
							paren_token: Default::default(),
							func: Box::new(Expr::Paren(ExprParen {
								attrs: Vec::new(),
								paren_token: Default::default(),
								expr: Box::new(Expr::Field(ExprField {
									attrs: Vec::new(),
									dot_token: Default::default(),
									base: Box::new(Expr::Path(ExprPath {
										attrs: Vec::new(),
										qself: None,
										path: path!(self),
									})),
									member: Member::Named(Ident::new("__drop", Span::call_site())),
								})),
							})),
							args: [Expr::Path(ExprPath {
								attrs: Vec::new(),
								qself: None,
								path: path!(instance),
							})]
							.into_iter()
							.collect(),
						}))],
					},
				})],
			})
		}

		pub fn impl_trait_for_dyn(&self) -> syn::Result<ItemImpl> {
			let subtable_generic_ident = Ident::new("__DynSubTables", Span::call_site());
			let repr_generic_ident = Ident::new("__DynRepr", Span::call_site());

			Ok(ItemImpl {
				attrs: Vec::new(),
				defaultness: None,
				unsafety: None,
				impl_token: Default::default(),
				generics: {
					let mut generics = self.dyntrait.generics.clone().strip_dyntable();

					let subtable_paths = match self.dyntrait.generics.where_clause.clone() {
						Some(where_clause) => {
							let supertable_graph = extract_supertables(
								self.dyntrait.supertraits.clone(),
								where_clause.predicates.clone(),
							)?;

							// Expand and deduplicate supertable graph
							let mut supertables = HashSet::<Path>::new();
							for supertable in supertable_graph {
								supertables.extend(supertable.iter().map(|p| p.clone()));
								supertables.insert(supertable.node);
							}

							supertables.insert(Path::from(PathSegment {
								ident: self.dyntrait.ident.clone(),
								arguments: path_arguments!(generic_params_into_args(
									self.dyntrait.generics.params.clone()
								)
								.collect()),
							}));

							supertables
						},
						None => {
							let mut supertables = HashSet::<Path>::new();
							supertables.insert(Path::from(PathSegment {
								ident: self.dyntrait.ident.clone(),
								arguments: path_arguments!(generic_params_into_args(
									self.dyntrait.generics.params.clone()
								)
								.collect()),
							}));

							supertables
						},
					};

					generics.params.extend(
						[subtable_generic_ident.clone(), repr_generic_ident.clone()].map(|p| {
							GenericParam::Type(TypeParam {
								attrs: Vec::new(),
								ident: p,
								colon_token: None,
								bounds: Punctuated::new(),
								eq_token: None,
								default: None,
							})
						}),
					);

					let where_clause = generics.where_clause.get_or_insert_with(|| WhereClause {
						where_token: Default::default(),
						predicates: Punctuated::new(),
					});

					where_clause
						.predicates
						.push(WherePredicate::Type(PredicateType {
							lifetimes: None,
							bounded_ty: Type::Path(TypePath {
								qself: None,
								path: Path::from(subtable_generic_ident.clone()),
							}),
							colon_token: Default::default(),
							bounds: subtable_paths
								.into_iter()
								.map(|path| {
									TypeParamBound::Trait(TraitBound {
										paren_token: None,
										modifier: TraitBoundModifier::None,
										lifetimes: None,
										path: path!(
											::dyntable::SubTable
												[GenericArgument::Type(vtable_path(path))]
										),
									})
								})
								.collect(),
						}));

					where_clause
						.predicates
						.push(WherePredicate::Type(PredicateType {
							lifetimes: None,
							bounded_ty: Type::Path(TypePath {
								qself: Some(QSelf {
									lt_token: Default::default(),
									gt_token: Default::default(),
									ty: Box::new(Type::Path(TypePath {
										qself: None,
										path: path!(__DynSubTables),
									})),
									as_token: Default::default(),
									position: 2,
								}),
								path: path!(::dyntable::VTable::Bounds),
							}),
							colon_token: Default::default(),
							bounds: self
								.dyntrait
								.supertraits
								.clone()
								.into_iter()
								.filter_map(|supertrait| match supertrait {
									TypeParamBound::Trait(TraitBound { path, .. }) => {
										Some(TypeParamBound::Trait(TraitBound {
											paren_token: None,
											modifier: TraitBoundModifier::None,
											lifetimes: None,
											path,
										}))
									},
									_ => None,
								})
								.collect(),
						}));

					where_clause
						.predicates
						.push(WherePredicate::Type(PredicateType {
							lifetimes: None,
							bounded_ty: Type::Path(TypePath {
								qself: None,
								path: Path::from(repr_generic_ident.clone()),
							}),
							colon_token: Default::default(),
							bounds: [
								TypeParamBound::Trait(TraitBound {
									paren_token: None,
									modifier: TraitBoundModifier::None,
									lifetimes: None,
									path: path!(
										::dyntable::VTableRepr[GenericArgument::Binding(Binding {
											ident: Ident::new("VTable", Span::call_site()),
											eq_token: Default::default(),
											ty: Type::Path(TypePath {
												qself: None,
												path: Path::from(subtable_generic_ident),
											}),
										})]
									),
								}),
								TypeParamBound::Trait(TraitBound {
									paren_token: None,
									modifier: TraitBoundModifier::Maybe(Default::default()),
									lifetimes: None,
									path: path!(::core::marker::Sized),
								}),
							]
							.into_iter()
							.collect(),
						}));

					generics
				},
				trait_: Some((
					None,
					Path::from(PathSegment {
						ident: self.dyntrait.ident.clone(),
						arguments: path_arguments!(generic_params_into_args(
							self.dyntrait.generics.params.clone()
						)
						.collect()),
					}),
					<Token![for]>::default(),
				)),
				self_ty: Box::new(Type::Path(TypePath {
					qself: None,
					path: path!(
						::dyntable::Dyn[GenericArgument::Type(Type::Path(TypePath {
							qself: None,
							path: Path::from(repr_generic_ident),
						}))]
					),
				})),
				brace_token: Default::default(),
				items: self
					.dyntrait
					.items
					.iter()
					.map(|item| match item {
						TraitItem::Method(method) => {
							let method = method.clone();

							let call = Expr::Call(ExprCall {
								attrs: Vec::new(),
								paren_token: Default::default(),
								func: Box::new(Expr::Paren(ExprParen {
									attrs: Vec::new(),
									paren_token: Default::default(),
									expr: Box::new(Expr::Field(ExprField {
										attrs: Vec::new(),
										dot_token: Default::default(),
										base: Box::new(Expr::Call(ExprCall {
											attrs: Vec::new(),
											paren_token: Default::default(),
											func: Box::new(Expr::Path(ExprPath {
												attrs: Vec::new(),
												qself: None,
												path: path!(::dyntable::SubTable[
													GenericArgument::Type(vtable_path(
														Path::from(PathSegment {
															ident: self.dyntrait.ident.clone(),
															arguments: path_arguments!(generic_params_into_args(
																self.dyntrait.generics.params.clone()
															).collect()),
														}),
													))
												]::subtable),
											})),
											args: [Expr::Reference(ExprReference {
												attrs: Vec::new(),
												and_token: Default::default(),
												raw: Default::default(),
												mutability: None,
												expr: Box::new(Expr::Unary(ExprUnary {
													attrs: Vec::new(),
													op: UnOp::Deref(Default::default()),
													expr: Box::new(Expr::Call(ExprCall {
														attrs: Vec::new(),
														paren_token: Default::default(),
														func: Box::new(Expr::Path(ExprPath {
															attrs: Vec::new(),
															qself: None,
															path: path!(
																::dyntable::__private::dyn_vtable
															),
														})),
														args: [Expr::Path(ExprPath {
															attrs: Vec::new(),
															qself: None,
															path: path!(self),
														})]
														.into_iter()
														.collect(),
													})),
												})),
											})]
											.into_iter()
											.collect(),
										})),
										member: Member::Named(method.sig.ident.clone()),
									})),
								})),
								args: method
									.sig
									.inputs
									.clone()
									.into_iter()
									.map(|arg| match arg {
										FnArg::Receiver(_) => Expr::Call(ExprCall {
											attrs: Vec::new(),
											paren_token: Default::default(),
											func: Box::new(Expr::Path(ExprPath {
												attrs: Vec::new(),
												qself: None,
												path: path!(::dyntable::__private::dyn_ptr),
											})),
											args: [Expr::Path(ExprPath {
												attrs: Vec::new(),
												qself: None,
												path: path!(self),
											})]
											.into_iter()
											.collect(),
										}),
										FnArg::Typed(PatType { pat, .. }) => {
											// I am not dealing with this
											Expr::Verbatim(pat.to_token_stream())
										},
									})
									.collect(),
							});

							let call_with_possible_deref = match &method.sig.output {
								ReturnType::Type(_, ty) => match &**ty {
									Type::Reference(TypeReference { mutability, .. }) => {
										Expr::Reference(ExprReference {
											attrs: Vec::new(),
											and_token: Default::default(),
											raw: Default::default(),
											mutability: mutability.clone(),
											expr: Box::new(Expr::Unary(ExprUnary {
												attrs: Vec::new(),
												op: UnOp::Deref(Default::default()),
												expr: Box::new(call),
											})),
										})
									},
									_ => call,
								},
								_ => call,
							};

							ImplItem::Method(ImplItemMethod {
								attrs: Vec::new(),
								vis: Visibility::Inherited,
								defaultness: None,
								sig: method.sig,
								block: Block {
									brace_token: Default::default(),
									stmts: vec![Stmt::Expr(Expr::Unsafe(ExprUnsafe {
										attrs: Vec::new(),
										unsafe_token: Default::default(),
										block: Block {
											brace_token: Default::default(),
											stmts: vec![Stmt::Expr(call_with_possible_deref)],
										},
									}))],
								},
							})
						},
						_ => unreachable!(), // already made sure this can't happen in build_vtable
					})
					.collect(),
			})
		}
	}
}

mod parse {
	use proc_macro2::Span;
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
		ItemTrait,
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
	pub struct AttrConf {
		pub repr: Option<Ident>,
		pub abi: Ident,
		pub drop: Drop,
	}

	#[derive(Debug, Clone)]
	pub enum Drop {
		None,
		FollowAbi,
		Abi(Ident),
	}

	impl Parse for AttrConf {
		fn parse(input: ParseStream) -> syn::Result<Self> {
			enum Option {
				Abi(Ident),
				Repr(std::option::Option<Ident>),
				DropAbi(std::option::Option<Ident>),
			}

			impl Parse for Option {
				fn parse(input: ParseStream) -> syn::Result<Self> {
					let ident = input.parse::<Ident>()?;
					match &ident.to_string() as &str {
						"abi" => {
							let _ = input.parse::<Token![=]>()?;
							Ok(Option::Abi(input.parse::<Ident>()?))
						},
						"repr" => {
							let _ = input.parse::<Token![=]>()?;
							let repr = input.parse::<Ident>()?;
							if repr.to_string() == "Rust" {
								Ok(Option::Repr(None))
							} else {
								Ok(Option::Repr(Some(repr)))
							}
						},
						"drop" => {
							let _ = input.parse::<Token![=]>()?;
							let abi = input.parse::<Ident>()?;
							if abi.to_string() == "none" {
								Ok(Option::DropAbi(None))
							} else {
								Ok(Option::DropAbi(Some(abi)))
							}
						},
						_ => Err(syn::Error::new(
							ident.span(),
							&format!("Unknown option: {}", ident),
						)),
					}
				}
			}

			let options = Punctuated::<Option, Token![,]>::parse_terminated(input)?;

			let mut conf = AttrConf {
				repr: None,
				abi: Ident::new("C", Span::call_site()),
				drop: Drop::FollowAbi,
			};

			for option in options {
				match option {
					Option::Repr(x) => conf.repr = x,
					Option::Abi(x) => conf.abi = x,
					Option::DropAbi(Some(x)) => conf.drop = Drop::Abi(x),
					Option::DropAbi(None) => conf.drop = Drop::None,
				}
			}

			Ok(conf)
		}
	}

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

	impl DynTrait {
		/// Strips out all dyntable information, leaving a normal `ItemTrait` struct
		pub fn strip_dyntable(self) -> ItemTrait {
			let Self {
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
			} = self;

			ItemTrait {
				attrs,
				vis,
				unsafety,
				auto_token: None,
				trait_token,
				ident,
				generics: generics.strip_dyntable(),
				colon_token,
				supertraits,
				brace_token,
				items,
			}
		}
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

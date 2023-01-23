//! Code generation and related utilities

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, ToTokens};
use syn::{
	punctuated::Punctuated,
	ConstParam,
	GenericParam,
	Lifetime,
	LifetimeDef,
	Token,
	TraitBound,
	Type,
	TypeParam,
	TypeParamBound,
	TypePtr,
	TypeReference,
};

use crate::parse::{
	Abi,
	DynTraitInfo,
	MethodEntry,
	MethodParam,
	MethodReceiver,
	Subtable,
	SubtableChildGraph,
	SubtableEntry,
	VTableEntry,
};

/// Generate expanded macro code from trait body
pub fn codegen(dyntrait: &DynTraitInfo) -> TokenStream {
	let vtable_ident = &dyntrait.vtable.name;
	let ident = &dyntrait.dyntrait.ident;
	let proxy_trait = format_ident!("__DynTable_{}", dyntrait.dyntrait.ident);
	let (impl_generics, ty_generics, where_clause) = dyntrait.generics.split_for_impl();

	let impl_generic_entries = dyntrait
		.generics
		.params
		.clone()
		.into_iter()
		.collect::<Vec<_>>();

	let impl_vt_generic_entries = dyntrait
		.generics
		.params
		.clone()
		.into_iter()
		.map(|mut param| {
			match &mut param {
				GenericParam::Lifetime(param) => {
					param.colon_token.get_or_insert_with(Default::default);
					param
						.bounds
						.insert(0, Lifetime::new("'__dyn_vtable", Span::call_site()));
				},
				GenericParam::Type(param) => {
					param.colon_token.get_or_insert_with(Default::default);
					param.bounds.insert(
						0,
						TypeParamBound::Lifetime(Lifetime::new("'__dyn_vtable", Span::call_site())),
					);
				},
				_ => {},
			};
			param
		})
		.collect::<Vec<_>>();

	let where_predicates = where_clause
		.into_iter()
		.flat_map(|clause| &clause.predicates)
		.collect::<Vec<_>>();

	let as_dyn_bounds = dyntrait
		.dyntrait
		.supertraits
		.iter()
		.filter(|supertrait| match supertrait {
			TypeParamBound::Lifetime(_) => false,
			TypeParamBound::Trait(TraitBound {
				path: superpath, ..
			}) => !dyntrait.entries.iter().any(|entry| match entry {
				VTableEntry::Subtable(SubtableEntry {
					subtable: Subtable { path, .. },
					..
				}) if path == superpath => true,
				_ => false,
			}),
		})
		.into_iter()
		.collect::<Vec<_>>();

	let trait_bounds = match dyntrait.dyntrait.supertraits.is_empty() {
		true => Vec::new(),
		false => vec![&dyntrait.dyntrait.supertraits],
	}
	.into_iter()
	.collect::<Vec<_>>();

	let trait_entries = dyntrait.entries.iter().flat_map(|entry| match entry {
		VTableEntry::Method(method) => Some(method),
		_ => None,
	});

	// Trait bounds that may be assumed to be applied to a
	// type associated with the generated VTable.
	let (vtable_bound_trait, vtable_bounds) = {
		let bounds = dyntrait
			.dyntrait
			.supertraits
			.iter()
			.filter_map(|supertrait| match supertrait {
				TypeParamBound::Trait(TraitBound { path, .. }) => Some(path),
				_ => None,
			})
			.collect::<Punctuated<_, Token![+]>>();

		match bounds.len() {
			0 => (None, quote::quote! { ::dyntable::__private::NoBounds }),
			1 => (None, bounds.to_token_stream()),
			_ => {
				let bound_ident = format_ident!("__DynBounds_{}", ident);
				let bounds = bounds.iter();
				(
					Some(quote::quote! {
						#[allow(non_camel_case_types)]
						trait #bound_ident #ty_generics: #(#bounds)+* {}
					}),
					bound_ident.to_token_stream(),
				)
			},
		}
	};

	let vtable_repr = dyntrait.vtable.repr.as_repr();

	let vtable_entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: Subtable { path, .. },
		}) => quote::quote! {
			#ident: <(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable
		},
		VTableEntry::Method(MethodEntry {
			unsafety,
			abi,
			fn_token,
			ident,
			receiver,
			inputs,
			output,
			..
		}) => {
			let self_ptr_type = receiver.pointer_type();

			let inputs = inputs
				.iter()
				.map(|MethodParam { ty, .. }| strip_references(ty.clone()));

			let output = match output {
				syn::ReturnType::Default => None,
				syn::ReturnType::Type(_, ty) => Some(strip_references(ty.as_ref().clone())),
			}
			.into_iter();

			quote::quote! {
				#ident: #unsafety #abi #fn_token (
					*#self_ptr_type ::core::ffi::c_void,
					#(#inputs),*
				) #( -> #output)*
			}
		},
	});

	let drop_abi = dyntrait
		.drop
		.as_ref()
		.map(Abi::as_abi)
		.into_iter()
		.collect::<Vec<_>>();
	let drop_fn_ident = drop_abi
		.iter()
		.map(|_| format_ident!("__DynDrop_{}", ident))
		.into_iter()
		.collect::<Vec<_>>();
	let vtable_phantom_generics = {
		let generics = impl_generic_entries
			.iter()
			.filter_map(|entry| match entry {
				GenericParam::Lifetime(LifetimeDef { lifetime, .. }) => {
					Some(quote::quote! { &#lifetime () })
				},
				GenericParam::Type(TypeParam { ident, .. }) => Some(ident.to_token_stream()),
				GenericParam::Const(_) => None,
			})
			.collect::<Vec<_>>();

		match generics.len() {
			1 => quote::quote! { #(#generics),* },
			// 0 OR more than 1 (0 params = `()`)
			_ => quote::quote! { (#(#generics),*) },
		}
	};

	let subtable_impls = dyntrait.entries.iter().filter_map(|entry| match entry {
		VTableEntry::Method(_) => None,
		VTableEntry::Subtable(SubtableEntry {
			ident: subtable_ident,
			subtable,
		}) => Some({
			let child_entries = subtable.flatten_child_graph().into_iter().map(
				|SubtableChildGraph {
				     parent: Subtable { path: parent, .. },
				     child: Subtable { path: child, .. },
				 }| {
					quote::quote! {
						impl #impl_generics
							::dyntable::SubTable<<(dyn #child + 'static) as ::dyntable::VTableRepr>::VTable>
						for #vtable_ident #ty_generics
						#where_clause {
							#[inline(always)]
							fn subtable(&self) ->
								&<(dyn #child + 'static) as ::dyntable::VTableRepr>::VTable
							{
								::dyntable::SubTable::<
									<(dyn #parent + 'static) as ::dyntable::VTableRepr>::VTable
								>::subtable(self).subtable()
							}
						}
					}
				},
			);

			let subtable_path = &subtable.path;

			quote::quote! {
				impl #impl_generics
					::dyntable::SubTable<<(dyn #subtable_path + 'static) as ::dyntable::VTableRepr>::VTable>
				for #vtable_ident #ty_generics
				#where_clause {
					#[inline(always)]
					fn subtable(&self) ->
						&<(dyn #subtable_path + 'static) as ::dyntable::VTableRepr>::VTable
					{
						&self.#subtable_ident
					}
				}

				#(#child_entries)*
			}
		}),
	});

	// Default entries for the generated VTable
	let impl_vtable_entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: Subtable { path, .. },
		}) => {
			quote::quote! {
				#ident: <__DynTarget as ::dyntable::DynTable<
					<(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable,
				>>::VTABLE
			}
		},
		VTableEntry::Method(MethodEntry {
			unsafety,
			abi,
			fn_token,
			ident: fn_ident,
			receiver,
			inputs,
			output,
			..
		}) => {
			let inputs = inputs.iter().map(|_| <Token![_]>::default());

			let output = match output {
				syn::ReturnType::Default => None,
				syn::ReturnType::Type(..) => Some(<Token![_]>::default()),
			}
			.into_iter();

			// functions that take self by value need a proxy function to
			// convert from a pointer to an owned Self
			let fn_path = match receiver {
				MethodReceiver::Reference(_) => quote::quote! { __DynTarget::#fn_ident },
				MethodReceiver::Value(_) => {
					let fn_generics = dyntrait
						.generics
						.params
						.clone()
						.into_iter()
						.map(|param| match param {
							GenericParam::Type(TypeParam { ident, .. }) => ident.to_token_stream(),
							GenericParam::Lifetime(LifetimeDef { lifetime, .. }) => {
								lifetime.to_token_stream()
							},
							GenericParam::Const(ConstParam { ident, .. }) => {
								ident.to_token_stream()
							},
						})
						.collect::<Vec<_>>();

					let fn_ident = format_ident!("__DynImpl_{}_{}", ident, fn_ident);
					quote::quote! { #fn_ident::<#(#fn_generics,)* __DynTarget> }
				},
			};

			quote::quote! {
				#fn_ident: unsafe {
					::core::intrinsics::transmute(
						#fn_path as
							#unsafety #abi #fn_token (
								_,
								#(#inputs),*
							) #( -> #output)*
					)
				}
			}
		},
	});

	// functions that take self by value need a proxy function to
	// convert from a pointer to an owned Self
	let proxy_fns = dyntrait.entries.iter().filter_map(|entry| match entry {
		VTableEntry::Method(MethodEntry {
			unsafety,
			abi,
			fn_token,
			ident: fn_ident,
			generics,
			receiver: MethodReceiver::Value(_),
			inputs,
			output,
		}) => {
			let proxy_fn_ident = format_ident!("__DynImpl_{}_{}", ident, fn_ident);
			let (_, _, fn_where_clause) = generics.split_for_impl();
			let param_list = MethodParam::params_safe(inputs.iter());
			let arg_list = MethodParam::idents_safe(inputs.iter());

			Some(quote::quote! {
				#[allow(non_snake_case)]
				#unsafety #abi #fn_token #proxy_fn_ident <
					#(#impl_generic_entries,)*
					__DynSelf: #ident #ty_generics,
				> (__dyn_self: *mut __DynSelf, #(#param_list),*) #output
				#fn_where_clause {
					<__DynSelf as #ident #ty_generics>::#fn_ident(
						unsafe { __dyn_self.read() },
						#(#arg_list),*
					)
				}
			})
		},
		_ => None,
	});

	let subtable_paths = dyntrait
		.entries
		.iter()
		.filter_map(|entry| match entry {
			VTableEntry::Method(_) => None,
			VTableEntry::Subtable(x) => Some(x),
		})
		.flat_map(|subtable| subtable.subtable.flatten())
		.map(|subtable| &subtable.path)
		.collect::<Vec<_>>();

	let dyn_impl_methods = dyntrait.entries.iter().filter_map(|entry| match entry {
		VTableEntry::Subtable(_) => None,
		VTableEntry::Method(MethodEntry {
			unsafety,
			abi,
			fn_token,
			ident: fn_ident,
			generics,
			receiver,
			inputs,
			output,
		}) => Some({
			// reborrow if a reference was returned, as it will be a pointer.
			let reborrow = match output {
				syn::ReturnType::Type(_, ty) => match ty.as_ref() {
					Type::Reference(TypeReference {
						and_token,
						mutability,
						..
					}) => quote::quote! { #and_token #mutability * },
					_ => TokenStream::new(),
				},
				_ => TokenStream::new(),
			};

			let (_, fn_ty_generics, fn_where_clause) = generics.split_for_impl();
			let param_list = MethodParam::params_safe(inputs.iter());
			let arg_list = MethodParam::idents_safe(inputs.iter());

			let code = match receiver {
				MethodReceiver::Reference(_) => quote::quote! {
					#reborrow (::dyntable::SubTable::<
						<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
					>::subtable(&*self.dyn_vtable()).#fn_ident)
						(self.dyn_ptr(), #(#arg_list),*)
				},
				MethodReceiver::Value(_) => quote::quote! {
					// call the function, the function will consider the pointer
					// to be by value
					let __dyn_result = #reborrow (::dyntable::SubTable::<
						<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
					>::subtable(&*self.dyn_vtable()).#fn_ident)
						(self.dyn_ptr(), #(#arg_list),*);
					// deallocate the pointer without dropping it
					self.dyn_dealloc();
					__dyn_result
				},
			};

			quote::quote! {
				#[inline(always)]
				#unsafety #abi #fn_token #fn_ident #fn_ty_generics (#receiver, #(#param_list),*) #output
				#fn_where_clause {
					unsafe { #code }
				}
			}
		}),
	});

	quote::quote! {
		trait #ident #ty_generics #(: #trait_bounds)*
		#where_clause {
			#(#trait_entries)*
		}

		#[allow(non_snake_case)]
		#vtable_repr
		struct #vtable_ident #ty_generics
		#where_clause {
			#(#vtable_entries,)*
			#(__drop: unsafe #drop_abi fn(*mut ::core::ffi::c_void),)*
			__generics: ::core::marker::PhantomData<#vtable_phantom_generics>
		}

		#vtable_bound_trait
		unsafe impl #impl_generics ::dyntable::VTable for #vtable_ident #ty_generics
		#where_clause {
			type Bounds = dyn #vtable_bounds;
		}

		impl #impl_generics ::dyntable::VTableRepr for dyn #ident #ty_generics
		#where_clause {
			type VTable = #vtable_ident #ty_generics;
		}

		impl #impl_generics ::dyntable::VTableRepr
		for dyn #ident #ty_generics + ::core::marker::Send
		#where_clause {
			type VTable = ::dyntable::__private::SendVTable<#vtable_ident #ty_generics>;
		}

		impl #impl_generics ::dyntable::VTableRepr
		for dyn #ident #ty_generics + ::core::marker::Sync
		#where_clause {
			type VTable = ::dyntable::__private::SyncVTable<#vtable_ident #ty_generics>;
		}

		impl #impl_generics ::dyntable::VTableRepr
		for dyn #ident #ty_generics + ::core::marker::Send + ::core::marker::Sync
		#where_clause {
			type VTable = ::dyntable::__private::SendSyncVTable<#vtable_ident #ty_generics>;
		}

		#(#subtable_impls)*

		#[allow(non_camel_case_types)]
		unsafe trait #proxy_trait<'v, V: 'v + ::dyntable::VTable> {
			const VTABLE: V;
			const STATIC_VTABLE: &'v V;
		}

		unsafe impl<
			'__dyn_vtable,
			#(#impl_vt_generic_entries,)*
			__DynTable,
		> ::dyntable::__private::DynTable2<'__dyn_vtable, #vtable_ident #ty_generics>
		for ::dyntable::__private::DynImplTarget<__DynTable, #vtable_ident #ty_generics>
		where
			#(#where_predicates,)*
			__DynTable: #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>,
		{
			const VTABLE: #vtable_ident #ty_generics = __DynTable::VTABLE;
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics = __DynTable::STATIC_VTABLE;
		}

		unsafe impl<
			'__dyn_vtable,
			#(#impl_vt_generic_entries,)*
			__DynTarget,
		> #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>
		for __DynTarget
		where
			#(#where_predicates,)*
			__DynTarget: #ident #ty_generics,
		{
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics =
				&<Self as #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>>::VTABLE;
			const VTABLE: #vtable_ident #ty_generics = #vtable_ident {
				#(#impl_vtable_entries,)*
				#(__drop: #drop_fn_ident::<__DynTarget>,)*
				__generics: ::core::marker::PhantomData,
			};
		}

		#(#proxy_fns)*

		#(
			#[allow(non_snake_case)]
			unsafe #drop_abi fn #drop_fn_ident<T>(ptr: *mut ::core::ffi::c_void) {
				::core::ptr::drop_in_place(ptr as *mut T);
			}

			impl #impl_generics ::dyntable::DropTable
			for #vtable_ident #ty_generics
			#where_clause {
				#[inline(always)]
				unsafe fn virtual_drop(&self, instance: *mut ::core::ffi::c_void) {
					(self.__drop)(instance)
				}
			}
		)*

		impl<
			#(#impl_generic_entries,)*
			__AsDyn,
		> #ident #ty_generics for __AsDyn
		where
			#(#where_predicates,)*
			__AsDyn: ::dyntable::AsDyn #(+ #as_dyn_bounds)*,
			<__AsDyn::Repr as ::dyntable::VTableRepr>::VTable:
				::dyntable::SubTable<#vtable_ident #ty_generics>
				#(+ ::dyntable::SubTable<
					<(dyn #subtable_paths + 'static) as ::dyntable::VTableRepr>::VTable
				>)*,
			#(<<__AsDyn::Repr as ::dyntable::VTableRepr>::VTable as ::dyntable::VTable>::Bounds: #trait_bounds,)*
		{
			#(#dyn_impl_methods)*
		}
	}
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

//! Code generation and related utilities

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, ToTokens};
use syn::{punctuated::Punctuated, GenericParam, Lifetime, Token, TraitBound, TypeParamBound};

use crate::parse::{
	DynTraitInfo,
	MethodEntry,
	MethodParam,
	MethodReceiver,
	Subtable,
	SubtableChildGraph,
	SubtableEntry,
	VTableEntry,
};

mod vtable;

/// Generate expanded macro code from trait body
pub fn codegen(dyntrait: &DynTraitInfo) -> TokenStream {
	let vtable_ident = &dyntrait.vtable.name;
	let vis = &dyntrait.vis;
	let ident = &dyntrait.dyntrait.ident;
	let trait_attrs = &dyntrait.dyntrait.attrs;
	let type_entries = &dyntrait.dyntrait.associated_types;
	let proxy_trait = format_ident!("__DynTrait_{}", dyntrait.dyntrait.ident);
	let (impl_generics, ty_generics, where_clause) = dyntrait.dyntrait.generics.split_for_impl();
	let (vt_impl_generics, vt_ty_generics, _) = dyntrait.vtable.generics.split_for_impl();
	let trait_vt_ty_generics = &dyntrait.dyntrait.vtable_ty_generics;

	let vtable_def = vtable::gen_vtable(dyntrait);
	let vtable_impl = vtable::gen_impl(dyntrait);

	let impl_generic_entries = dyntrait
		.vtable
		.generics
		.params
		.clone()
		.into_iter()
		.collect::<Vec<_>>();

	let impl_vt_generic_entries = dyntrait
		.vtable
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
			let (_, fn_ty_generics, fn_where_clause) = generics.split_for_impl();

			let param_list = MethodParam::params_safe(inputs.iter());
			let arg_list = MethodParam::idents_safe(inputs.iter());

			let code = match receiver {
				MethodReceiver::Reference(_) => quote::quote! {
					(::dyntable::SubTable::<
						<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
					>::subtable(&*self.dyn_vtable()).#fn_ident)(
						::dyntable::DynSelf::from_raw(self.dyn_ptr()),
						#(#arg_list),*
					)
				},
				MethodReceiver::Value(_) => quote::quote! {
					// call the function, the function will consider the pointer
					// to be by value
					let __dyn_result = (::dyntable::SubTable::<
						<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
					>::subtable(&*self.dyn_vtable()).#fn_ident)(
						self.dyn_ptr(),
						#(#arg_list),*
					);
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
		#(#trait_attrs)*
		#vis trait #ident #impl_generics #(: #trait_bounds)*
		#where_clause {
			#(#type_entries)*
			#(#trait_entries)*
		}

		#vtable_def

		#vtable_bound_trait
		unsafe impl #vt_impl_generics ::dyntable::VTable for #vtable_ident #vt_ty_generics
		#where_clause {
			type Bounds = dyn #vtable_bounds;
		}

		impl #vt_impl_generics ::dyntable::VTableRepr for dyn #ident #trait_vt_ty_generics
		#where_clause {
			type VTable = #vtable_ident #vt_ty_generics;
		}

		impl #vt_impl_generics ::dyntable::VTableRepr
		for dyn #ident #trait_vt_ty_generics + ::core::marker::Send
		#where_clause {
			type VTable = ::dyntable::__private::SendVTable<#vtable_ident #vt_ty_generics>;
		}

		impl #vt_impl_generics ::dyntable::VTableRepr
		for dyn #ident #trait_vt_ty_generics + ::core::marker::Sync
		#where_clause {
			type VTable = ::dyntable::__private::SyncVTable<#vtable_ident #vt_ty_generics>;
		}

		impl #vt_impl_generics ::dyntable::VTableRepr
		for dyn #ident #trait_vt_ty_generics + ::core::marker::Send + ::core::marker::Sync
		#where_clause {
			type VTable = ::dyntable::__private::SendSyncVTable<#vtable_ident #vt_ty_generics>;
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
			__DynTrait,
		> ::dyntable::__private::DynTraitProxy<'__dyn_vtable, #vtable_ident #vt_ty_generics>
		for ::dyntable::__private::DynImplTarget<__DynTrait, #vtable_ident #vt_ty_generics>
		where
			#(#where_predicates,)*
			__DynTrait: #proxy_trait<'__dyn_vtable, #vtable_ident #vt_ty_generics>,
		{
			const VTABLE: #vtable_ident #vt_ty_generics = __DynTrait::VTABLE;
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #vt_ty_generics = __DynTrait::STATIC_VTABLE;
		}

		#vtable_impl

		impl<
			#(#impl_generic_entries,)*
			__AsDyn,
		> #ident #ty_generics for __AsDyn
		where
			#(#where_predicates,)*
			__AsDyn: ::dyntable::AsDyn #(+ #as_dyn_bounds)*,
			<__AsDyn::Repr as ::dyntable::VTableRepr>::VTable:
				::dyntable::SubTable<#vtable_ident #vt_ty_generics>
				#(+ ::dyntable::SubTable<
					<(dyn #subtable_paths + 'static) as ::dyntable::VTableRepr>::VTable
				>)*,
			#(<<__AsDyn::Repr as ::dyntable::VTableRepr>::VTable as ::dyntable::VTable>::Bounds: #trait_bounds,)*
		{
			#(#dyn_impl_methods)*
		}
	}
}

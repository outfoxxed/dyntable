//! Code generation and related utilities

use proc_macro2::TokenStream;
use quote::{format_ident, ToTokens};
use syn::{
	punctuated::Punctuated,
	FnArg,
	PatType,
	Receiver,
	Signature,
	Token,
	TraitBound,
	TraitItemMethod,
	Type,
	TypeParamBound,
	TypePtr,
	TypeReference,
};

use crate::parse::{DynTraitInfo, Subtable, SubtableEntry, VTableEntry};

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
	let where_predicates = where_clause
		.into_iter()
		.flat_map(|clause| &clause.predicates)
		.collect::<Vec<_>>();

	let trait_bounds = match dyntrait.dyntrait.supertraits.is_empty() {
		true => None,
		false => Some(&dyntrait.dyntrait.supertraits),
	}
	.into_iter();

	let trait_entries = dyntrait.entries.iter().flat_map(|entry| match entry {
		VTableEntry::Method(method) => Some(method),
		_ => None,
	});

	// Trait bounds that may be assumed to be applied to a
	// type associated with the generated VTable.
	let vtable_bounds = if dyntrait.dyntrait.supertraits.is_empty() {
		quote::quote! { ::dyntable::__private::NoBounds }
	} else {
		dyntrait
			.dyntrait
			.supertraits
			.iter()
			.filter_map(|supertrait| match supertrait {
				TypeParamBound::Trait(TraitBound { path, .. }) => Some(path),
				_ => None,
			})
			.collect::<Punctuated<_, Token![+]>>()
			.to_token_stream()
	};

	let vtable_entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: Subtable { path, .. },
		}) => quote::quote! {
			#ident: <(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable
		},
		VTableEntry::Method(TraitItemMethod {
			sig:
				Signature {
					unsafety,
					abi,
					fn_token,
					ident,
					inputs,
					variadic,
					output,
					..
				},
			..
		}) => {
			let inputs = inputs.iter().map(|arg| match arg {
				FnArg::Receiver(Receiver { mutability, .. }) => {
					// receiver enforced to be a reference in parse/dyntrait.rs

					let constness = match mutability {
						Some(_) => None,
						None => Some(<Token![const]>::default()),
					};

					quote::quote! { *#mutability #constness ::core::ffi::c_void }
				},
				FnArg::Typed(PatType { ty, .. }) => {
					strip_references(ty.as_ref().clone()).to_token_stream()
				},
			});
			let output = match output {
				syn::ReturnType::Default => None,
				syn::ReturnType::Type(_, ty) => Some(strip_references(ty.as_ref().clone())),
			}
			.into_iter();

			quote::quote! {
				#ident: #unsafety #abi #fn_token (
					#(#inputs,)* #variadic
				) #( -> #output)*
			}
		},
	});

	// Default entries for the generated VTable
	let impl_vtable_entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(subtable) => {
			let entry_name = &subtable.ident;
			quote::quote! {
				#entry_name: <__DynTarget as ::dyntable::DynTable<
					<(dyn #ident #ty_generics + 'static) as ::dyntable::VTableRepr>::VTable,
				>>::VTABLE
			}
		},
		VTableEntry::Method(method) => {
			let Signature {
				unsafety,
				abi,
				fn_token,
				ident,
				inputs,
				variadic,
				output,
				..
			} = &method.sig;

			let inputs = inputs.iter().map(|_| <Token![_]>::default());

			let output = match output {
				syn::ReturnType::Default => None,
				syn::ReturnType::Type(..) => Some(<Token![_]>::default()),
			}
			.into_iter();

			quote::quote! {
				#ident: unsafe {
					::core::intrinsics::transmute(
						__DynTarget::#ident as
							#unsafety #abi #fn_token (
								#(#inputs,)* #variadic
							) #( -> #output)*
					)
				}
			}
		},
	});

	quote::quote! {
		trait #ident #ty_generics #(: #trait_bounds)*
		#where_clause {
			#(#trait_entries)*
		}

		struct #vtable_ident #ty_generics
		#where_clause {
			#(#vtable_entries,)*
		}

		impl #impl_generics ::dyntable::VTable for #vtable_ident #ty_generics
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

		#[allow(non_camel_case_types)]
		unsafe trait #proxy_trait<'v, V: 'v + ::dyntable::VTable> {
			const VTABLE: V;
			const STATIC_VTABLE: &'v V;
		}

		unsafe impl<
			'__dyn_vtable,
			#(#impl_generic_entries + '__dyn_vtable,)*
			__DynTarget,
		> ::dyntable::__private::DynTable2<'__dyn_vtable, #vtable_ident #ty_generics>
		for ::dyntable::__private::DynImplTarget<__DynTarget, #vtable_ident #ty_generics>
		where
			#(#where_predicates,)*
			__DynTarget: #proxy_trait<'__dyn_vtable, #vtable_ident #ty_generics>,
		{
			const VTABLE: #vtable_ident #ty_generics = __DynTarget::VTABLE;
			const STATIC_VTABLE: &'__dyn_vtable #vtable_ident #ty_generics = __DynTarget::STATIC_VTABLE;
		}

		unsafe impl<
				'__dyn_vtable,
			#(#impl_generic_entries + '__dyn_vtable,)*
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
			};
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

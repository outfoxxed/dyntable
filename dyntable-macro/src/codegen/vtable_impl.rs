use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{punctuated::Punctuated, Token, TraitBound, TypeParamBound};

use crate::parse::DynTraitInfo;

/// Implement VTable and VTableRepr for the vtable and trait respectively
pub fn impl_vtable_trait(dyntrait: &DynTraitInfo) -> TokenStream {
	let bounds = if dyntrait.dyntrait.supertraits.is_empty() {
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

	let vtable_ident = &dyntrait.vtable.name;
	let ident = &dyntrait.dyntrait.ident;
	let (impl_generics, ty_generics, where_clause) = dyntrait.generics.split_for_impl();

	quote::quote! {
		impl #impl_generics ::dyntable::VTable for #vtable_ident #ty_generics #where_clause {
			type Bounds = dyn #bounds;
		}

		impl #impl_generics ::dyntable::VTableRepr for dyn #ident #ty_generics #where_clause {
			type VTable = #vtable_ident #ty_generics;
		}
		impl #impl_generics ::dyntable::VTableRepr for dyn #ident #ty_generics + ::core::marker::Send
		#where_clause {
			type VTable = ::dyntable::_private::SendVTable<#vtable_ident #ty_generics>;
		}
		impl #impl_generics ::dyntable::VTableRepr for dyn #ident #ty_generics + ::core::marker::Sync
		#where_clause {
			type VTable = ::dyntable::_private::SyncVTable<#vtable_ident #ty_generics>;
		}
	}
}

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, ToTokens};
use syn::{ConstParam, GenericParam, Lifetime, LifetimeDef, Token, TypeParam, TypeParamBound};

use crate::parse::{
	DynTraitInfo,
	MethodEntry,
	MethodParam,
	MethodReceiver,
	Subtable,
	SubtableEntry,
	TraitInfo,
	VTableEntry,
	VTableInfo,
};

pub fn gen_impl(
	dyntrait @ DynTraitInfo {
		dyntrait: TraitInfo { ident, .. },
		vtable: VTableInfo {
			name: vtable_ident, ..
		},
		generics: trait_generics,
		drop: drop_abi,
		embed_layout,
		..
	}: &DynTraitInfo,
) -> TokenStream {
	let proxy_trait = format_ident!("__DynTrait_{}", dyntrait.dyntrait.ident);
	let (impl_generics, ty_generics, where_clause) = trait_generics.split_for_impl();

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

	let entries = dyntrait.entries.iter().map(|entry| match entry {
		VTableEntry::Subtable(SubtableEntry {
			ident,
			subtable: Subtable { path, .. },
		}) => {
			quote::quote! {
				#ident: <__DynTarget as ::dyntable::DynTrait<
					<(dyn #path + 'static) as ::dyntable::VTableRepr>::VTable,
				>>::VTABLE
			}
		},
		VTableEntry::Method(method) => gen_method_entry(dyntrait, method),
	});

	let proxy_fns = dyntrait.entries.iter().filter_map(|entry| match entry {
		VTableEntry::Method(
			method @ MethodEntry {
				receiver: MethodReceiver::Value(_),
				..
			},
		) => Some(gen_self_proxy(dyntrait, method)),
		_ => None,
	});

	let (drop_ident, drop_abi) = match drop_abi.as_ref() {
		Some(drop_abi) => {
			let ident = format_ident!("__DynDrop_{}", ident);
			let abi = drop_abi.as_abi();

			(
				Some(ident).into_iter().collect::<Vec<_>>(),
				Some(abi).into_iter(),
			)
		},
		None => (Vec::new(), None.into_iter()),
	};

	let embed_layout = match embed_layout {
		true => Some(TokenStream::new()),
		false => None,
	}
	.into_iter()
	.collect::<Vec<_>>();

	quote::quote! {
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
				#(__drop: #drop_ident::<__DynTarget>,)*
				#(__layout: ::dyntable::alloc::MemoryLayout::new::<__DynTarget>(), #embed_layout)* // embed_layout is a marker
				#(#entries,)*
				__generics: ::core::marker::PhantomData,
			};
		}

		#(#proxy_fns)*

		// drop implementation
		#(
			#[allow(non_snake_case)]
			unsafe #drop_abi fn #drop_ident<T>(ptr: *mut ::core::ffi::c_void) {
				::core::ptr::drop_in_place(ptr as *mut T)
			}

			unsafe impl #impl_generics ::dyntable::AssociatedDrop
			for #vtable_ident #ty_generics
			#where_clause {
				#[inline(always)]
				unsafe fn virtual_drop(&self, instance: *mut ::core::ffi::c_void) {
					(self.__drop)(instance)
				}
			}
		)*

		#(#embed_layout // marker, no code generated
			unsafe impl #impl_generics ::dyntable::AssociatedLayout
			for #vtable_ident #ty_generics
			#where_clause {
				#[inline(always)]
				fn virtual_layout(&self) -> ::dyntable::alloc::MemoryLayout {
					self.__layout
				}
			}
		)*
	}
}

/// Generate a proxy function for a dyntrait method that takes
/// self by value.
fn gen_self_proxy(
	DynTraitInfo {
		dyntrait: TraitInfo { ident, .. },
		generics: trait_generics,
		..
	}: &DynTraitInfo,
	MethodEntry {
		unsafety,
		abi,
		fn_token,
		ident: fn_ident,
		generics,
		inputs,
		output,
		..
	}: &MethodEntry,
) -> TokenStream {
	let proxy_fn_ident = format_ident!("__DynImpl_{}_{}", ident, fn_ident);
	let (_, ty_generics, _) = trait_generics.split_for_impl();
	let (_, _, fn_where_clause) = generics.split_for_impl();
	let param_list = MethodParam::params_safe(inputs.iter());
	let arg_list = MethodParam::idents_safe(inputs.iter());

	let impl_generic_entries = trait_generics
		.params
		.clone()
		.into_iter()
		.collect::<Vec<_>>();

	quote::quote! {
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
	}
}

/// Generate a vtable entry for a solid base type
fn gen_method_entry(
	DynTraitInfo {
		dyntrait: TraitInfo { ident, .. },
		generics: trait_generics,
		..
	}: &DynTraitInfo,
	MethodEntry {
		unsafety,
		abi,
		fn_token,
		ident: fn_ident,
		receiver,
		inputs,
		output,
		..
	}: &MethodEntry,
) -> TokenStream {
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
			let fn_generics = trait_generics
				.params
				.clone()
				.into_iter()
				.map(|param| match param {
					GenericParam::Type(TypeParam { ident, .. }) => ident.to_token_stream(),
					GenericParam::Lifetime(LifetimeDef { lifetime, .. }) => {
						lifetime.to_token_stream()
					},
					GenericParam::Const(ConstParam { ident, .. }) => ident.to_token_stream(),
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
}
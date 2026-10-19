//! `#[derive(NativeLayout)]`: a native layout for a component without `Reflect`, from
//! `core::mem::offset_of!`. Fields must be public and of bridge field types; no `repr(C)`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Visibility, parse_macro_input, spanned::Spanned};

#[proc_macro_derive(NativeLayout)]
pub fn derive_native_layout(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(t) => t.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new(input.generics.span(), "NativeLayout: generic components are not supported"));
    }
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(input.span(), "NativeLayout: only structs have a native layout"));
    };
    let mut fields = Vec::new();
    let members: Vec<(String, proc_macro2::TokenStream, &syn::Field)> = match &data.fields {
        Fields::Named(f) => f
            .named
            .iter()
            .map(|f| {
                let id = f.ident.clone().unwrap();
                (id.to_string(), quote!(#id), f)
            })
            .collect(),
        Fields::Unnamed(f) => f
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let idx = syn::Index::from(i);
                (i.to_string(), quote!(#idx), f)
            })
            .collect(),
        Fields::Unit => return Err(syn::Error::new(input.span(), "NativeLayout: a unit struct has no fields; use it in With/Without")),
    };
    for (label, member, f) in &members {
        if !matches!(f.vis, Visibility::Public(_)) {
            return Err(syn::Error::new(
                f.span(),
                format!("NativeLayout: field `{label}` must be `pub`, as `offset_of!` outside this module needs"),
            ));
        }
        let ty = &f.ty;
        fields.push(quote! {
            ::bevy_ts_native::layout::FieldLayout {
                name: ::std::string::String::from(#label),
                ty: <#ty as ::bevy_ts_native::NativeField>::field_type(),
                offset: ::core::mem::offset_of!(#name, #member),
            }
        });
    }
    Ok(quote! {
        impl ::bevy_ts_native::NativeLayout for #name {
            fn native_layout() -> ::bevy_ts_native::layout::ComponentLayout {
                ::bevy_ts_native::layout::ComponentLayout {
                    size: ::core::mem::size_of::<#name>(),
                    align: ::core::mem::align_of::<#name>(),
                    fields: ::std::vec![#(#fields),*],
                }
            }
        }
    })
}

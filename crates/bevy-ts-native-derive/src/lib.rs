//! `#[derive(NativeLayout)]`: a native layout for a component without `Reflect`, from
//! `core::mem::offset_of!`. Fields must be public and of bridge field types; no `repr(C)`.

use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::visit_mut::VisitMut;
use syn::{Data, DeriveInput, Fields, FnArg, ImplItem, ItemImpl, Pat, ReturnType, Type, Visibility, parse_macro_input, spanned::Spanned};

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

/// `#[derive(HostRecord)]`: a struct of host values (numbers, bool, Entity, vectors, records)
/// that crosses as an argument or result of an exported method, field by field.
#[proc_macro_derive(HostRecord)]
pub fn derive_host_record(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return syn::Error::new(input.span(), "HostRecord: only structs").to_compile_error().into();
    };
    let Fields::Named(fields) = &data.fields else {
        return syn::Error::new(input.span(), "HostRecord: fields must be named").to_compile_error().into();
    };
    let h = quote!(::bevy_ts_native::bevy_ts::host);
    let mut kinds = Vec::new();
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for f in &fields.named {
        let id = f.ident.clone().unwrap();
        let ty = &f.ty;
        let ts = camel(&id.to_string());
        kinds.push(quote_spanned!(ty.span()=> (::std::string::String::from(#ts), <#ty as #h::HostValue>::kind())));
        reads.push(quote_spanned!(ty.span()=> #id: <#ty as #h::HostValue>::read(r)?));
        writes.push(quote!(#h::HostValue::write(&self.#id, w);));
    }
    let ts_name = name.to_string();
    quote! {
        impl #h::HostValue for #name {
            fn kind() -> #h::Kind {
                #h::Kind::Record(#ts_name, ::std::vec![#(#kinds),*])
            }
            fn read(r: &mut #h::SlotReader) -> ::core::result::Result<Self, ::std::string::String> {
                ::core::result::Result::Ok(#name { #(#reads),* })
            }
            fn write(&self, w: &mut #h::SlotWriter) {
                #(#writes)*
            }
        }
    }
    .into()
}

fn camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = !out.is_empty();
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

struct StaticLifetimes;

impl VisitMut for StaticLifetimes {
    fn visit_lifetime_mut(&mut self, l: &mut syn::Lifetime) {
        *l = syn::Lifetime::new("'static", l.span());
    }
}

/// `#[export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` of a read-only
/// `SystemParam`: every `pub fn(&self, ...)` becomes a method of the TS type `Name`, called
/// synchronously from TS systems as one host call. Register it with `app.ts_param::<P>()`.
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ts = None;
    let mut module = None;
    let parser = syn::meta::parser(|meta| {
        let v: syn::LitStr = meta.value()?.parse()?;
        if meta.path.is_ident("ts") {
            ts = Some(v);
        } else if meta.path.is_ident("module") {
            module = Some(v);
        } else {
            return Err(meta.error("export takes ts = \"Name\" and module = \"bevy-ts/<module>\""));
        }
        Ok(())
    });
    parse_macro_input!(attr with parser);
    let item = parse_macro_input!(item as ItemImpl);
    match expand_export(&item, ts, module) {
        Ok(t) => quote!(#item #t).into(),
        Err(e) => {
            let e = e.to_compile_error();
            quote!(#item #e).into()
        }
    }
}

fn expand_export(item: &ItemImpl, ts: Option<syn::LitStr>, module: Option<syn::LitStr>) -> syn::Result<proc_macro2::TokenStream> {
    let ts = ts.ok_or_else(|| syn::Error::new(item.span(), "export needs ts = \"Name\", the TS type"))?;
    let module = module.ok_or_else(|| syn::Error::new(item.span(), "export needs module = \"bevy-ts/<module>\""))?;
    if item.trait_.is_some() {
        return Err(syn::Error::new(item.span(), "export goes on an inherent impl of the param"));
    }
    let mut st = (*item.self_ty).clone();
    StaticLifetimes.visit_type_mut(&mut st);
    let h = quote!(::bevy_ts_native::bevy_ts::host);
    let bevy = quote!(::bevy_ts_native::bevy);
    let mut methods = Vec::new();
    for it in &item.items {
        let ImplItem::Fn(f) = it else { continue };
        if !matches!(f.vis, Visibility::Public(_)) {
            continue;
        }
        let sig = &f.sig;
        let method = &sig.ident;
        match sig.inputs.first() {
            Some(FnArg::Receiver(r)) if r.reference.is_some() && r.mutability.is_none() => {}
            _ => return Err(syn::Error::new(sig.span(), format!("exported method {method} must take &self"))),
        }
        let mut decls = Vec::new();
        let mut specs = Vec::new();
        let mut call_args = Vec::new();
        let mut slices = 0usize;
        for arg in sig.inputs.iter().skip(1) {
            let FnArg::Typed(pt) = arg else { unreachable!() };
            let Pat::Ident(pi) = &*pt.pat else {
                return Err(syn::Error::new(pt.pat.span(), "exported method arguments must be plain names"));
            };
            let name = &pi.ident;
            let ts_arg = camel(&name.to_string());
            let local = format_ident!("__{}", name);
            match &*pt.ty {
                Type::Reference(r) if matches!(&*r.elem, Type::Slice(_)) => {
                    let Type::Slice(sl) = &*r.elem else { unreachable!() };
                    let elem = &sl.elem;
                    let mutable = r.mutability.is_some();
                    slices += 1;
                    if slices > 4 {
                        return Err(syn::Error::new(pt.span(), "an exported method takes at most 4 slice arguments"));
                    }
                    let n = name.to_string();
                    if mutable {
                        decls.push(quote_spanned!(elem.span()=> let #local: &mut [#elem] = #h::slice_mut::<#elem>(&mut **__sl.next().ok_or("missing slice")?, #n)?;));
                    } else {
                        decls.push(quote_spanned!(elem.span()=> let #local: &[#elem] = #h::slice_ref::<#elem>(&**__sl.next().ok_or("missing slice")?, #n)?;));
                    }
                    specs.push(quote_spanned!(elem.span()=> #h::ArgSpec::Slice(#h::SliceSpec { name: #ts_arg, view: <#elem as #h::HostElem>::view(), mutable: #mutable })));
                }
                Type::Reference(r) => {
                    return Err(syn::Error::new(r.span(), format!("argument {name}: only &[T] and &mut [T] references cross to TS")));
                }
                ty => {
                    decls.push(quote_spanned!(ty.span()=> let #local: #ty = <#ty as #h::HostValue>::read(__r)?;));
                    specs.push(quote_spanned!(ty.span()=> #h::ArgSpec::Value(#ts_arg, <#ty as #h::HostValue>::kind())));
                }
            }
            call_args.push(quote!(#local));
        }
        let ret = match &sig.output {
            ReturnType::Default => quote!(()),
            ReturnType::Type(_, t) => quote!(#t),
        };
        let ts_method = camel(&method.to_string());
        methods.push(quote! {{
            fn __call(
                __state: &mut #h::ParamState,
                __world: &#bevy::prelude::World,
                __r: &mut #h::SlotReader,
                __w: &mut #h::SlotWriter,
                __slices: &mut [&mut [u8]],
            ) -> ::core::result::Result<(), ::std::string::String> {
                let __st = __state
                    .downcast_mut::<#bevy::ecs::system::SystemState<#st>>()
                    .ok_or("exported param state has the wrong type")?;
                let __p = __st.get(__world).map_err(|e| e.to_string())?;
                #[allow(unused_mut, unused_variables)]
                let mut __sl = __slices.iter_mut();
                #(#decls)*
                let __out: #ret = __p.#method(#(#call_args),*);
                #h::HostValue::write(&__out, __w);
                ::core::result::Result::Ok(())
            }
            #h::MethodSpec {
                name: ::std::string::String::from(#ts_method),
                args: ::std::vec![#(#specs),*],
                ret: <#ret as #h::HostValue>::kind(),
                call: __call,
            }
        }});
    }
    Ok(quote! {
        impl ::bevy_ts_native::ExportParam for #st {
            fn spec() -> #h::ParamSpec {
                fn __init(world: &mut #bevy::prelude::World) -> #h::ParamState {
                    ::std::boxed::Box::new(#bevy::ecs::system::SystemState::<#st>::new(world))
                }
                #h::ParamSpec {
                    ts: #ts,
                    module: #module,
                    rust: ::core::any::type_name::<#st>(),
                    init: __init,
                    methods: ::std::vec![#(#methods),*],
                }
            }
        }
    })
}

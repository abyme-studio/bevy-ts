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
        // A newtype's one field is `value` in TS, as layout_reflect names it.
        Fields::Unnamed(u) => u
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let idx = syn::Index::from(i);
                let label = if u.unnamed.len() == 1 { "value".to_string() } else { i.to_string() };
                (label, quote!(#idx), f)
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
/// that crosses as an argument or result of an exported method, field by field; or a fieldless
/// enum, which crosses as its variant names (`"Any" | "Visible"`).
#[proc_macro_derive(HostRecord)]
pub fn derive_host_record(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let data = match &input.data {
        Data::Struct(data) => data,
        Data::Enum(e) => return host_enum(name, e).unwrap_or_else(|e| e.to_compile_error()).into(),
        Data::Union(_) => return syn::Error::new(input.span(), "HostRecord: only structs and fieldless enums").to_compile_error().into(),
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
        // A record has a fixed layout of slots; a string's length varies.
        if is_string(ty) || option_of(ty).is_some_and(is_string) {
            return syn::Error::new(ty.span(), format!("HostRecord: field `{id}` is a string; strings cross as method arguments and results, not record fields"))
                .to_compile_error()
                .into();
        }
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

fn host_enum(name: &syn::Ident, e: &syn::DataEnum) -> syn::Result<proc_macro2::TokenStream> {
    let h = quote!(::bevy_ts_native::bevy_ts::host);
    let mut names = Vec::new();
    let mut idents = Vec::new();
    for v in &e.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(syn::Error::new(v.span(), format!("HostRecord: variant {} has fields; an enum crosses only when fieldless", v.ident)));
        }
        names.push(v.ident.to_string());
        idents.push(&v.ident);
    }
    if idents.is_empty() {
        return Err(syn::Error::new(name.span(), "HostRecord: an enum needs a variant"));
    }
    let index = (0..idents.len()).map(|i| i as f64);
    let index2 = index.clone();
    let ts_name = name.to_string();
    Ok(quote! {
        impl #h::HostValue for #name {
            fn kind() -> #h::Kind {
                #h::Kind::Enum(#ts_name, ::std::vec![#(#names),*])
            }
            fn read(r: &mut #h::SlotReader) -> ::core::result::Result<Self, ::std::string::String> {
                let k = r.slot()?;
                #(if k == #index { return ::core::result::Result::Ok(#name::#idents); })*
                ::core::result::Result::Err(::std::format!("{k} is not a {}", #ts_name))
            }
            fn write(&self, w: &mut #h::SlotWriter) {
                w.push(match self { #(#name::#idents => #index2),* });
            }
        }
    })
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

/// Whether `t` is `String`, `&str` or `str`.
fn is_string(t: &Type) -> bool {
    match t {
        Type::Reference(r) => is_string(&r.elem),
        Type::Path(p) => p.qself.is_none() && p.path.segments.last().is_some_and(|s| (s.ident == "String" || s.ident == "str") && s.arguments.is_none()),
        _ => false,
    }
}

/// `&str` when `t` is written `&str`.
fn is_str_ref(t: &Type) -> bool {
    matches!(t, Type::Reference(r) if r.mutability.is_none() && matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")))
}

/// `T` when `t` is written `Option<T>`.
fn option_of(t: &Type) -> Option<&Type> {
    let Type::Path(p) = t else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else { return None };
    match a.args.first()? {
        syn::GenericArgument::Type(t) if a.args.len() == 1 => Some(t),
        _ => None,
    }
}

/// `T` when `t` is written `Result<T, E>`.
fn result_ok(t: &Type) -> Option<&Type> {
    let Type::Path(p) = t else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else { return None };
    match a.args.first()? {
        syn::GenericArgument::Type(ok) if a.args.len() == 2 => Some(ok),
        _ => None,
    }
}

struct StaticLifetimes;

impl VisitMut for StaticLifetimes {
    fn visit_lifetime_mut(&mut self, l: &mut syn::Lifetime) {
        *l = syn::Lifetime::new("'static", l.span());
    }
}

/// A Bevy param spelling, normalised: no whitespace, every path by its last segment. Mirrors
/// `bevy_ts::host::normalise_spelling` (a proc-macro cannot call it); the registry's
/// `claimed` lookup in native_writes.rs exercises both.
fn normalise_spelling(s: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
        } else {
            out.push_str(word.rsplit("::").next().unwrap_or(""));
            word.clear();
            out.push(c);
        }
    }
    out.push_str(word.rsplit("::").next().unwrap_or(""));
    out
}

/// `#[export(ts = "Name", module = "bevy-ts/<module>")]` on an `impl` of a `SystemParam`: every
/// `pub fn(&self, ...)` or `pub fn(&mut self, ...)` becomes a method of the TS type `Name`,
/// called synchronously from TS systems as one host call. Register it with
/// `app.ts_param::<P>()`. `stands_for = "ResMut<Time<Virtual>>"` makes a TS param written with
/// that Bevy spelling this export.
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut ts = None;
    let mut module = None;
    let mut stands_for = None;
    let parser = syn::meta::parser(|meta| {
        let v: syn::LitStr = meta.value()?.parse()?;
        if meta.path.is_ident("ts") {
            ts = Some(v);
        } else if meta.path.is_ident("module") {
            module = Some(v);
        } else if meta.path.is_ident("stands_for") {
            let n = normalise_spelling(&v.value());
            if !(n.starts_with("Res<") || n.starts_with("ResMut<")) || !n.ends_with('>') {
                return Err(syn::Error::new(v.span(), "stands_for names one Res<..> or ResMut<..> spelling"));
            }
            stands_for = Some(syn::LitStr::new(&n, v.span()));
        } else {
            return Err(meta.error("export takes ts = \"Name\", module = \"bevy-ts/<module>\" and stands_for = \"ResMut<..>\""));
        }
        Ok(())
    });
    parse_macro_input!(attr with parser);
    let item = parse_macro_input!(item as ItemImpl);
    match expand_export(&item, ts, module, stands_for) {
        Ok(t) => quote!(#item #t).into(),
        Err(e) => {
            let e = e.to_compile_error();
            quote!(#item #e).into()
        }
    }
}

fn expand_export(item: &ItemImpl, ts: Option<syn::LitStr>, module: Option<syn::LitStr>, stands_for: Option<syn::LitStr>) -> syn::Result<proc_macro2::TokenStream> {
    let ts = ts.ok_or_else(|| syn::Error::new(item.span(), "export needs ts = \"Name\", the TS type"))?;
    let module = module.ok_or_else(|| syn::Error::new(item.span(), "export needs module = \"bevy-ts/<module>\""))?;
    if item.trait_.is_some() {
        return Err(syn::Error::new(item.span(), "export goes on an inherent impl of the param"));
    }
    let stands_for = match stands_for {
        Some(s) => quote!(::core::option::Option::Some(#s)),
        None => quote!(::core::option::Option::None),
    };
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
            Some(FnArg::Receiver(r)) if r.reference.is_some() => {}
            _ => return Err(syn::Error::new(sig.span(), format!("exported method {method} must take &self or &mut self"))),
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
                // `&str` and `Option<&str>`: read as an owned string, lent to the method.
                ty if is_str_ref(ty) || option_of(ty).is_some_and(is_str_ref) => {
                    let optional = !is_str_ref(ty);
                    let owned = if optional { quote!(::core::option::Option<::std::string::String>) } else { quote!(::std::string::String) };
                    let held = format_ident!("__{}_owned", name);
                    decls.push(quote_spanned!(ty.span()=> let #held: #owned = <#owned as #h::HostValue>::read(__r)?;));
                    if optional {
                        decls.push(quote_spanned!(ty.span()=> let #local: #ty = #held.as_deref();));
                    } else {
                        decls.push(quote_spanned!(ty.span()=> let #local: #ty = #held.as_str();));
                    }
                    specs.push(quote_spanned!(ty.span()=> #h::ArgSpec::Value(#ts_arg, <#owned as #h::HostValue>::kind())));
                }
                Type::Reference(r) => {
                    return Err(syn::Error::new(r.span(), format!("argument {name}: only &str, &[T] and &mut [T] references cross to TS")));
                }
                ty => {
                    decls.push(quote_spanned!(ty.span()=> let #local: #ty = <#ty as #h::HostValue>::read(__r)?;));
                    specs.push(quote_spanned!(ty.span()=> #h::ArgSpec::Value(#ts_arg, <#ty as #h::HostValue>::kind())));
                }
            }
            call_args.push(quote!(#local));
        }
        // `Result<T, E>` returns `T` to TS and throws `E` (its Display) as an exception.
        let (ret, fallible) = match &sig.output {
            ReturnType::Default => (quote!(()), false),
            ReturnType::Type(_, t) => match result_ok(t) {
                Some(ok) => (quote!(#ok), true),
                None => (quote!(#t), false),
            },
        };
        let invoke = if fallible {
            quote!(__p.#method(#(#call_args),*).map_err(|e| e.to_string())?)
        } else {
            quote!(__p.#method(#(#call_args),*))
        };
        let ts_method = camel(&method.to_string());
        methods.push(quote! {{
            fn __call(
                __state: &mut #h::ParamState,
                __world: #bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell<'_>,
                __ticks: #h::RunTicks,
                __r: &mut #h::SlotReader,
                __w: &mut #h::SlotWriter,
                __slices: &mut [&mut [u8]],
            ) -> ::core::result::Result<(), ::std::string::String> {
                // SAFETY: the host calls this only from a TS run that holds the world
                // exclusively; JS is suspended, so no other param or view is read or written
                // during the call; the state came from `host::init`, which ran `init_access`,
                // and registration refused unbounded writes; slices overlapping a column the
                // call could alias were refused before this call.
                #[allow(unused_mut)]
                let mut __p = unsafe { #h::fetch::<#st>(__state, __world, __ticks) }?;
                #[allow(unused_mut, unused_variables)]
                let mut __sl = __slices.iter_mut();
                #(#decls)*
                let __out: #ret = #invoke;
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
                #h::ParamSpec {
                    ts: #ts,
                    module: #module,
                    rust: ::core::any::type_name::<#st>(),
                    stands_for: #stands_for,
                    init: #h::init::<#st>,
                    methods: ::std::vec![#(#methods),*],
                }
            }
        }
    })
}

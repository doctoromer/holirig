use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};

use crate::auto_dispatch::AutoDispatch;

mod auto_dispatch;

struct AutoDispatchAttrs {
    type_info_fn: Option<syn::Path>,
}

impl Parse for AutoDispatchAttrs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { type_info_fn: None });
        }
        let name: syn::Ident = input.parse()?;
        if name != "type_info" {
            return Err(syn::Error::new(name.span(), "expected `type_info`"));
        }
        input.parse::<syn::Token![=]>()?;
        Ok(Self {
            type_info_fn: Some(input.parse()?),
        })
    }
}

#[proc_macro_attribute]
pub fn auto_dispatch(attrs: TokenStream, body: TokenStream) -> TokenStream {
    let attrs: AutoDispatchAttrs = syn::parse(attrs).unwrap();
    let mut auto_dispatch: AutoDispatch = syn::parse2(body.into()).unwrap();
    auto_dispatch.type_info_fn = attrs.type_info_fn;
    quote! { #auto_dispatch }.into()
}

use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::ToTokens;
use quote::quote;
use syn::Ident;
use syn::parse::Parse;
use syn::spanned::Spanned;

type DispId = i32;

#[derive(Debug, PartialEq, Eq, Clone)]
enum PropertyType {
    IUnknown,
    IDispatch,
    Bstr,
    Bool,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F64,
    Variant,
    Other(String),
}

impl ToTokens for PropertyType {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let result = match self {
            PropertyType::IUnknown => quote! { windows::core::IUnknown },
            PropertyType::IDispatch => quote! { IDispatch },
            PropertyType::Bstr => quote! { BSTR },
            PropertyType::Bool => quote! { bool },
            PropertyType::U16 => quote! { u16 },
            PropertyType::I16 => quote! { i16 },
            PropertyType::U32 => quote! { u32 },
            PropertyType::I32 => quote! { i32 },
            PropertyType::U64 => quote! { u64 },
            PropertyType::I64 => quote! { i64 },
            PropertyType::F64 => quote! { f64 },
            PropertyType::Variant => quote! { windows::Win32::System::Variant::VARIANT },
            PropertyType::Other(other) => {
                let ident = Ident::new(other, Span::call_site());
                quote! { #ident }
            }
        };
        tokens.extend(result);
    }
}

enum FuncType {
    Getter,
    Setter,
    Method,
}

struct DispatchFunc {
    property_type: Option<PropertyType>,
    get_func: Option<syn::ImplItemFn>,
    set_func: Option<syn::ImplItemFn>,
    method_func: Option<syn::ImplItemFn>,
}

pub struct AutoDispatch {
    target: String,
    dispid_to_name: BTreeMap<DispId, String>,
    dispid_to_const: BTreeMap<DispId, Ident>,
    dispatch_funcs: BTreeMap<DispId, DispatchFunc>,
    pub(crate) type_info_fn: Option<syn::Path>,
}

fn get_simple_path(path: &syn::Path) -> syn::Result<&syn::PathSegment> {
    if path.segments.len() == 1 {
        Ok(path.segments.first().unwrap())
    } else {
        Err(syn::Error::new(path.span(), "Expected simple path"))
    }
}

impl AutoDispatch {
    fn new(target: String) -> Self {
        Self {
            target,
            dispid_to_name: BTreeMap::new(),
            dispid_to_const: BTreeMap::new(),
            dispatch_funcs: BTreeMap::new(),
            type_info_fn: None,
        }
    }

    fn parse_id_attribute(attr: &syn::Attribute) -> syn::Result<Option<DispId>> {
        if let syn::Meta::List(list) = &attr.meta
            && let Some(ident) = list.path.get_ident()
            && ident.to_string().as_str() == "id"
            && let syn::Lit::Int(id) = syn::parse::Parser::parse2(
                |input: syn::parse::ParseStream<'_>| input.parse::<syn::Lit>(),
                list.tokens.clone(),
            )?
        {
            Ok(Some(id.base10_parse::<DispId>()?))
        } else {
            Ok(None)
        }
    }

    fn parse_func_type_attribute(attr: &syn::Attribute) -> syn::Result<Option<FuncType>> {
        if let syn::Meta::Path(path) = &attr.meta {
            let name = get_simple_path(path)?.ident.to_string();
            let result = match name.as_str() {
                "getter" => FuncType::Getter,
                "setter" => FuncType::Setter,
                _ => {
                    return Err(syn::Error::new(
                        attr.span(),
                        format!("Unknown attribute: {name}"),
                    ));
                }
            };
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    fn parse_attributes(&mut self, func: &syn::ImplItemFn) -> syn::Result<(DispId, FuncType)> {
        let mut dispid = None;
        let mut func_type = None;
        for attr in &func.attrs {
            if let Some(id) = Self::parse_id_attribute(attr)? {
                if dispid.is_some() {
                    return Err(syn::Error::new(attr.span(), "Duplicated id attribute"));
                }
                dispid = Some(id);
            }
            if let Some(func_type_attr) = Self::parse_func_type_attribute(attr)? {
                if func_type.is_some() {
                    return Err(syn::Error::new(
                        attr.span(),
                        "Duplicated property type attribute",
                    ));
                }
                func_type = Some(func_type_attr);
            }
        }

        let Some(dispid) = dispid else {
            return Err(syn::Error::new(
                func.span(),
                "Expected id attribute: #[id(<dispid>)]",
            ));
        };
        let func_type = func_type.unwrap_or(FuncType::Method);

        let func_name = func.sig.ident.to_string();
        if let Some(name) = self.dispid_to_name.get(&dispid)
            && name != &func_name
        {
            return Err(syn::Error::new(
                func.span(),
                format!("Duplicated id for functions `{name}`, `{func_name}`"),
            ));
        }
        let name = func.sig.ident.to_string();

        self.dispid_to_const.insert(
            dispid,
            Ident::new(
                format!("{}_DISPID", name.to_uppercase()).as_str(),
                func.sig.ident.span(),
            ),
        );
        self.dispid_to_name.insert(dispid, name);

        Ok((dispid, func_type))
    }

    fn parse_property_type(property_type: &syn::Type) -> syn::Result<Option<PropertyType>> {
        match property_type {
            syn::Type::Path(type_path) => {
                let segment = get_simple_path(&type_path.path)?.ident.to_string();
                let property_type = match segment.as_str() {
                    "IUnknown" => PropertyType::IUnknown,
                    "IDispatch" => PropertyType::IDispatch,
                    "BSTR" => PropertyType::Bstr,
                    "bool" => PropertyType::Bool,
                    "u16" => PropertyType::U16,
                    "i16" => PropertyType::I16,
                    "u32" => PropertyType::U32,
                    "i32" => PropertyType::I32,
                    "u64" => PropertyType::U64,
                    "i64" => PropertyType::I64,
                    "f64" => PropertyType::F64,
                    "VARIANT" => PropertyType::Variant,
                    _ => PropertyType::Other(segment),
                };
                Ok(Some(property_type))
            }
            syn::Type::Tuple(type_tuple) => {
                if type_tuple.elems.is_empty() {
                    Ok(None)
                } else {
                    Err(syn::Error::new(type_tuple.span(), "Unsupported type"))
                }
            }
            _ => Err(syn::Error::new(property_type.span(), "Unsupported type")),
        }
    }

    fn parse_return_type(func: &syn::ImplItemFn) -> syn::Result<Option<PropertyType>> {
        let func_name = func.sig.ident.to_string();

        let syn::ReturnType::Type(_, output_type) = &func.sig.output else {
            return Err(syn::Error::new(
                func.span(),
                format!("Missing output type for function `{func_name}`"),
            ));
        };

        let syn::Type::Path(type_path) = output_type.as_ref() else {
            return Err(syn::Error::new(
                output_type.span(),
                format!("Invalid output type for function `{func_name}`"),
            ));
        };

        let segment = get_simple_path(&type_path.path)?;
        let syn::PathArguments::AngleBracketed(segment_args) = &segment.arguments else {
            return Err(syn::Error::new(
                segment.span(),
                format!("Invalid output type for function `{func_name}`"),
            ));
        };

        let args: Vec<_> = segment_args.args.iter().cloned().collect();

        let [
            syn::GenericArgument::Type(return_type),
            syn::GenericArgument::Type(error_type),
        ] = &args[..]
        else {
            return Err(syn::Error::new(
                segment_args.args.span(),
                format!("Invalid output type for function `{func_name}`"),
            ));
        };

        if let syn::Type::Path(type_path) = error_type {
            let err_type = get_simple_path(&type_path.path)?.ident.to_string();
            if err_type.as_str() != "HRESULT" {
                return Err(syn::Error::new(
                    segment_args.args.span(),
                    format!(
                        "Err type of return type of `{func_name}` must be HRESULT, but it is `{err_type}`"
                    ),
                ));
            }
        } else {
            return Err(syn::Error::new(
                segment_args.args.span(),
                "Invalid err type of return type",
            ));
        }

        Self::parse_property_type(return_type)
    }

    fn parse_input_type(func: &syn::ImplItemFn) -> syn::Result<Option<PropertyType>> {
        let args: Vec<_> = func.sig.inputs.iter().cloned().collect();

        let (receiver, maybe_arg) = match &args[..] {
            [syn::FnArg::Receiver(receiver), syn::FnArg::Typed(arg)] => (receiver, Some(arg)),
            [syn::FnArg::Receiver(receiver)] => (receiver, None),
            _ => {
                return Ok(None);
            }
        };

        match &receiver.reference {
            None | Some((_, Some(_))) => {
                return Err(syn::Error::new(
                    receiver.span(),
                    "self parameter must be `&self`",
                ));
            }
            Some((_, None)) => {}
        }
        if receiver.mutability.is_some() {
            return Err(syn::Error::new(
                receiver.span(),
                "self parameter must not be mutable",
            ));
        }

        if receiver.colon_token.is_some() {
            return Err(syn::Error::new(
                receiver.span(),
                "self parameter must not be typed",
            ));
        }

        if let Some(arg) = maybe_arg {
            Self::parse_property_type(&arg.ty)
        } else {
            Ok(None)
        }
    }

    fn parse_function(&mut self, func: &syn::ImplItemFn) -> syn::Result<()> {
        let (id, func_type) = self.parse_attributes(func)?;
        let return_type = Self::parse_return_type(func)?;
        let input_type = Self::parse_input_type(func)?;

        let mut processed_func = func.clone();
        processed_func.attrs = vec![];

        match func_type {
            FuncType::Getter => {
                let (Some(return_type), None) = (return_type, input_type) else {
                    return Err(syn::Error::new(
                        func.span(),
                        "Function signature doesn't match to a getter function",
                    ));
                };

                processed_func.sig.ident = Ident::new(
                    format!("get_{}", processed_func.sig.ident).as_str(),
                    processed_func.sig.ident.span(),
                );

                if let Some(dispatch_func) = self.dispatch_funcs.get_mut(&id) {
                    if dispatch_func.get_func.is_some() {
                        return Err(syn::Error::new(
                            func.span(),
                            "Duplicated get_property function",
                        ));
                    }
                    if dispatch_func.property_type.clone().unwrap() != return_type {
                        return Err(syn::Error::new(func.span(), "Mismatch in property type"));
                    }
                    dispatch_func.get_func = Some(processed_func.clone());
                } else {
                    self.dispatch_funcs.insert(
                        id,
                        DispatchFunc {
                            property_type: Some(return_type),
                            get_func: Some(processed_func.clone()),
                            set_func: None,
                            method_func: None,
                        },
                    );
                }
            }
            FuncType::Setter => {
                let (Some(input_type), None) = (input_type, return_type) else {
                    return Err(syn::Error::new(
                        func.span(),
                        "Function signature doesn't match to a getter function",
                    ));
                };

                processed_func.sig.ident = Ident::new(
                    format!("set_{}", processed_func.sig.ident).as_str(),
                    processed_func.sig.ident.span(),
                );

                if let Some(dispatch_func) = self.dispatch_funcs.get_mut(&id) {
                    if dispatch_func.set_func.is_some() {
                        return Err(syn::Error::new(
                            func.span(),
                            "Duplicated set_property function",
                        ));
                    }
                    if dispatch_func.property_type.as_ref().unwrap() != &input_type {
                        return Err(syn::Error::new(func.span(), "Mismatch in property type"));
                    }
                    dispatch_func.set_func = Some(processed_func.clone());
                } else {
                    self.dispatch_funcs.insert(
                        id,
                        DispatchFunc {
                            property_type: Some(input_type),
                            get_func: None,
                            set_func: Some(processed_func.clone()),
                            method_func: None,
                        },
                    );
                }
            }
            FuncType::Method => {
                if let Some(dispatch_func) = self.dispatch_funcs.get_mut(&id) {
                    if dispatch_func.method_func.is_some() {
                        return Err(syn::Error::new(
                            func.span(),
                            "Duplicated get_property function",
                        ));
                    }
                    dispatch_func.method_func = Some(processed_func.clone());
                } else {
                    self.dispatch_funcs.insert(
                        id,
                        DispatchFunc {
                            property_type: return_type,
                            get_func: None,
                            set_func: None,
                            method_func: Some(processed_func),
                        },
                    );
                }
            }
        }
        Ok(())
    }

    fn generate_dispids(&self) -> proc_macro2::TokenStream {
        let mut dispids: Vec<_> = self.dispid_to_const.iter().collect();
        dispids.sort();

        let dispids_consts: Vec<_> = dispids
            .iter()
            .map(|(dispid, const_name)| {
                quote! { const #const_name: i32 = #dispid; }
            })
            .collect();

        quote! {
            #(#dispids_consts)*
        }
    }

    fn generate_inner_funcs(&self) -> proc_macro2::TokenStream {
        let mut dispatch_funcs: Vec<_> = self.dispatch_funcs.iter().collect();
        dispatch_funcs.sort_by_key(|(id, _)| **id);

        let mut result = quote! {};
        for (_, funcs) in dispatch_funcs {
            if let Some(get_func) = &funcs.get_func {
                result.extend(quote! { #get_func });
            }
            if let Some(set_func) = &funcs.set_func {
                result.extend(quote! { #set_func });
            }
            if let Some(method_func) = &funcs.method_func {
                result.extend(quote! { #method_func });
            }
        }
        result
    }

    fn generate_get_ids_of_names(&self) -> proc_macro2::TokenStream {
        let match_arms = self.dispid_to_name.iter().map(|(dispid, name)| {
            let const_name = &self.dispid_to_const[dispid];
            quote! {
                #name => Self::#const_name,
            }
        });

        quote! {
            fn GetIDsOfNames(
                &self,
                _riid: *const windows::core::GUID,
                rgsznames: *const windows::core::PCWSTR,
                cnames: u32,
                _lcid: u32,
                rgdispid: *mut i32,
            ) -> windows::core::Result<()> {
                unsafe {
                    if rgsznames.is_null() || rgdispid.is_null() {
                        return Err(windows::Win32::Foundation::E_INVALIDARG.into());
                    }

                    for i in 0..cnames {
                        let name_ptr = *rgsznames.add(i as usize);
                        let name = name_ptr.to_string()?;

                        let dispid = match name.as_str() {
                            #(#match_arms)*
                            _ => return Err(windows::Win32::Foundation::DISP_E_MEMBERNOTFOUND.into()),
                        };

                        *rgdispid.add(i as usize) = dispid;
                    }

                    Ok(())
                }
            }
        }
    }

    fn generate_invoke(&self) -> proc_macro2::TokenStream {
        let mut dispid_consts: Vec<_> = self.dispid_to_const.iter().collect();

        dispid_consts.sort_by_key(|(dispid, _)| **dispid);

        let match_arms: Vec<_> = dispid_consts
            .into_iter()
            .map(|(dispid, const_name)| {
                let dispatch_func = &self.dispatch_funcs[dispid];

                let property_get = if let Some(get_func) = &dispatch_func.get_func {
                    let func_name = &get_func.sig.ident;

                    quote! {
                        if wflags.contains(windows::Win32::System::Com::DISPATCH_PROPERTYGET) {
                            if !pvarresult.is_null() {
                                *pvarresult = self.#func_name().map_err(windows_core::Error::from_hresult)?.into();
                            }
                            Ok(())
                        } else
                    }
                } else {
                    quote! {}
                };

                let property_set = if let Some(set_func) = &dispatch_func.set_func {
                    let func_name = &set_func.sig.ident;

                    quote! {
                        if wflags.contains(windows::Win32::System::Com::DISPATCH_PROPERTYPUT) {
                            if pdispparams.is_null() {
                                return Err(windows::Win32::Foundation::E_INVALIDARG.into());
                            }
                            let params = &*pdispparams;
                            if params.cArgs == 0 || params.rgvarg.is_null() {
                                return Err(windows::Win32::Foundation::DISP_E_PARAMNOTFOUND.into());
                            }
                            let value = &*params.rgvarg;
                            let value = value
                                .try_into()
                                .or(Err(windows_core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)))?;

                            self.#func_name(value).map_err(windows_core::Error::from_hresult)
                        } else
                    }
                } else {
                    quote! {}
                };

                let method = if let Some(method_func) = &dispatch_func.method_func {
                    let func_name = &method_func.sig.ident;

                    let params: Vec<_> = method_func
                        .sig
                        .inputs
                        .iter()
                        .skip(1)
                        .enumerate()
                        .map(|(index, arg)| {
                            if let syn::FnArg::Typed(arg) = arg {
                                let name = Ident::new(format!("val{}", index + 1).as_str(), arg.span());
                                (name, arg.ty.as_ref().clone())
                            } else {
                                panic!();
                            }
                        })
                        .collect();

                    let check_args = if !params.is_empty() {
                        let params_len = params.len() as u32;
                        quote! {
                            let params = &*pdispparams;
                            if params.cArgs != #params_len || params.rgvarg.is_null() {
                                return Err(windows::Win32::Foundation::DISP_E_PARAMNOTFOUND.into());
                            }
                        }
                    } else {
                        quote! {}
                    };

                    let unwrap_args: Vec<_> = params
                        .iter()
                        .enumerate()
                        .map(|(index, (name, param_type))| {
                            let index = params.len() - 1 - index;

                            // Check if this parameter is a VARIANT.
                            // VARIANT parameters need to be cloned, not converted
                            let is_variant = if let syn::Type::Path(type_path) = param_type {
                                if let Some(segment) = type_path.path.segments.last() {
                                    segment.ident == "VARIANT"
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            if is_variant {
                                quote! {
                                    let #name = &*params.rgvarg.add(#index);
                                    let #name: #param_type = #name.clone();
                                }
                            } else {
                                quote! {
                                    let #name = &*params.rgvarg.add(#index);
                                    let #name: #param_type = #name
                                        .try_into()
                                        .or(Err(windows_core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG)))?;
                                }
                            }
                        })
                        .collect();

                    let arg_names: Vec<_> = params.iter().map(|(name, _)| name.clone()).collect();

                    let func_call = quote! {
                        self.#func_name(#(#arg_names),*).map_err(windows_core::Error::from_hresult)?
                    };

                    let func_call = if dispatch_func.property_type.is_some() {
                        quote! {
                            if !pvarresult.is_null() {
                                *pvarresult = #func_call.into();
                            }
                        }
                    } else {
                        quote! { #func_call; }
                    };

                    quote! {
                        if wflags.contains(windows::Win32::System::Com::DISPATCH_METHOD) {
                            if pdispparams.is_null() {
                                return Err(windows::Win32::Foundation::E_INVALIDARG.into());
                            }
                            #check_args
                            #(#unwrap_args)*
                            #func_call
                            Ok(())
                        } else
                    }
                } else {
                    quote! {}
                };

                quote! {
                    Self::#const_name => {
                        #property_get
                        #property_set
                        #method
                        {
                            Err(windows::Win32::Foundation::E_INVALIDARG.into())
                        }
                    }
                }
            })
            .collect();

        quote! {
            fn Invoke(
                &self,
                dispidmember: i32,
                _riid: *const windows::core::GUID,
                _lcid: u32,
                wflags: windows::Win32::System::Com::DISPATCH_FLAGS,
                pdispparams: *const windows::Win32::System::Com::DISPPARAMS,
                pvarresult: *mut windows::Win32::System::Variant::VARIANT,
                _pexcepinfo: *mut windows::Win32::System::Com::EXCEPINFO,
                _puargerr: *mut u32,
            ) -> windows::core::Result<()> {
                unsafe {
                    match dispidmember {
                        #(#match_arms)*
                        _ => Err(windows::Win32::Foundation::DISP_E_MEMBERNOTFOUND.into())
                    }
                }
            }
        }
    }
}

impl Parse for AutoDispatch {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let item: syn::Item = input.parse()?;

        let syn::Item::Impl(impl_block) = item else {
            return Err(syn::Error::new(item.span(), "Expected impl block"));
        };

        if impl_block.trait_.is_some() {
            return Err(syn::Error::new(
                impl_block.span(),
                "impl <trait> for <type> { ... } is not allowed, only impl <type> { ...  }",
            ));
        }

        let target = if let syn::Type::Path(type_path) = impl_block.self_ty.as_ref() {
            get_simple_path(&type_path.path)?.ident.to_string()
        } else {
            return Err(syn::Error::new(
                impl_block.self_ty.span(),
                "Invalid type name to implement",
            ));
        };

        let mut auto_dispatch = AutoDispatch::new(target);

        for inner_item in impl_block.items {
            let syn::ImplItem::Fn(func) = inner_item else {
                return Err(syn::Error::new(
                    inner_item.span(),
                    "Only functions are allowed",
                ));
            };

            auto_dispatch.parse_function(&func)?;
        }

        Ok(auto_dispatch)
    }
}

impl ToTokens for AutoDispatch {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let impl_struct_ident =
            Ident::new(format!("{}_Impl", self.target).as_str(), Span::call_site());

        let dispids_consts = self.generate_dispids();
        let inner_funcs_impl = self.generate_inner_funcs();
        let get_ids_of_names = self.generate_get_ids_of_names();
        let invoke = self.generate_invoke();

        let (get_type_info_count, get_type_info) = if let Some(ref path) = self.type_info_fn {
            (
                quote! {
                    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
                        Ok(1)
                    }
                },
                quote! {
                    fn GetTypeInfo(&self, itinfo: u32, _lcid: u32) -> windows::core::Result<windows::Win32::System::Com::ITypeInfo> {
                        if itinfo != 0 {
                            return Err(windows::Win32::Foundation::DISP_E_BADINDEX.into());
                        }
                        #path()
                    }
                },
            )
        } else {
            (
                quote! {
                    fn GetTypeInfoCount(&self) -> windows::core::Result<u32> {
                        Ok(0)
                    }
                },
                quote! {
                    fn GetTypeInfo(&self, _itinfo: u32, _lcid: u32) -> windows::core::Result<windows::Win32::System::Com::ITypeInfo> {
                        Err(windows::Win32::Foundation::E_NOTIMPL.into())
                    }
                },
            )
        };

        let result = quote! {
            impl #impl_struct_ident {
                #dispids_consts
                #inner_funcs_impl
            }

            #[allow(clippy::not_unsafe_ptr_arg_deref)]
            impl IDispatch_Impl for #impl_struct_ident {
                #get_type_info_count
                #get_type_info
                #get_ids_of_names
                #invoke
            }
        };
        tokens.extend(result);
    }
}

use syn::parse::Parser;
use syn::{parse_quote, ItemStruct, Path};

use quote::quote;

use super::{Crates, MacroArgs};

pub(crate) fn grpc_request(
    macro_args: MacroArgs,
    mut item_struct: ItemStruct,
) -> proc_macro2::TokenStream {
    let module = macro_args.module.to_string();
    let identity = macro_args.identity;
    let name = &item_struct.ident;

    let Crates { serde, std } = macro_args.crates;

    // add derive GrpcMessageData
    let grpc_message_body = quote! {
        impl crate::common::remote::grpc::message::GrpcMessageData for #name {
            fn identity<'a>() -> std::borrow::Cow<'a, str> {
                #identity.into()
            }
        }
    };

    // add derive GrpcRequestMessage
    let grpc_message_request = quote! {

        impl crate::common::remote::grpc::message::GrpcRequestMessage for #name {

            fn header(&self, key: &str) -> Option<&String>{
                self.headers.get(key)
            }

            fn headers(&self) -> &#std::collections::HashMap<String, String> {
                &self.headers
            }

            fn take_headers(&mut self) -> #std::collections::HashMap<String, String> {
                #std::mem::take(&mut self.headers)
            }

            fn add_headers(&mut self, map: #std::collections::HashMap<String, String>) {
                self.headers.extend(map.into_iter());
            }

            fn request_id(&self) -> Option<&String> {
                self.request_id.as_ref()
            }

            fn module(&self) -> &str {
                #module
            }
        }

    };

    // add field
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let headers_field = syn::Field::parse_named
            .parse2(quote! {
                pub headers: #std::collections::HashMap<String, String>
            })
            .unwrap();
        let request_id_field = syn::Field::parse_named
            .parse2(quote! {
                pub request_id: Option<String>
            })
            .unwrap();

        fields.named.push(headers_field);
        fields.named.push(request_id_field);
    }

    let derive_paths: Vec<Path> = vec![
        syn::parse_quote! { #serde::Deserialize },
        syn::parse_quote! { #serde::Serialize },
        syn::parse_quote! { Clone },
        syn::parse_quote! { Debug },
        syn::parse_quote! { Default },
    ];

    // add derive
    item_struct.attrs.push(parse_quote! {
        #[derive(#(#derive_paths),*)]

    });
    item_struct.attrs.push(parse_quote!(
        #[serde(rename_all = "camelCase")]
    ));

    match macro_args.module {
        super::Module::Naming => naming_request(&mut item_struct),
        super::Module::Config => config_request(&mut item_struct),
        _ => {}
    }

    quote! {
        #item_struct
        #grpc_message_body
        #grpc_message_request
    }
}

/// Naming request fields
fn naming_request(item_struct: &mut ItemStruct) {
    // add fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let namespace_field = syn::Field::parse_named
            .parse2(quote! {
                pub namespace: Option<String>
            })
            .unwrap();
        let service_name_field = syn::Field::parse_named
            .parse2(quote! {
                pub service_name: Option<String>
            })
            .unwrap();

        let group_name_field = syn::Field::parse_named
            .parse2(quote! {
                pub group_name: Option<String>
            })
            .unwrap();

        fields.named.push(namespace_field);
        fields.named.push(service_name_field);
        fields.named.push(group_name_field);
    }
}

/// Config request fields
fn config_request(item_struct: &mut ItemStruct) {
    // add fields
    if let syn::Fields::Named(ref mut fields) = item_struct.fields {
        let namespace_field = syn::Field::parse_named
            .parse2(quote! {
                #[serde(rename = "tenant")]
                pub namespace: Option<String>
            })
            .unwrap();
        let data_id_field = syn::Field::parse_named
            .parse2(quote! {
                pub data_id: Option<String>
            })
            .unwrap();

        let group_field = syn::Field::parse_named
            .parse2(quote! {
                pub group: Option<String>
            })
            .unwrap();

        fields.named.push(namespace_field);
        fields.named.push(data_id_field);
        fields.named.push(group_field);
    }
}

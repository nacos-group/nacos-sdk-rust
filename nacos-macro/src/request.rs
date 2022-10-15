use syn::parse::Parser;
use syn::{parse_quote, ItemStruct, Path};

use quote::quote;

use crate::{Crates, MacroArgs};

pub(crate) fn grpc_request(
    macro_args: MacroArgs,
    mut item_struct: ItemStruct,
) -> proc_macro2::TokenStream {
    let identity = macro_args.identity;
    let name = &item_struct.ident;

    let Crates { serde, std } = macro_args.crates;

    // add derive GrpcMessageData
    let grpc_message_body = quote! {
        impl crate::naming::grpc::message::GrpcMessageData for #name {
            fn identity<'a>() -> std::borrow::Cow<'a, str> {
                #identity.into()
            }
        }
    };

    // add derive GrpcRequestMessage
    let grpc_message_request = quote! {

        impl crate::naming::grpc::message::GrpcRequestMessage for #name {

            fn header(&self, key: &str) -> Option<&String>{
                self.headers.get(key)
            }

            fn headers(&self) -> &#std::collections::HashMap<String, String> {
                &self.headers
            }

            fn take_headers(&mut self) -> #std::collections::HashMap<String, String> {
                #std::mem::take(&mut self.headers)
            }

            fn request_id(&self) -> Option<&String> {
                self.request_id.as_ref()
            }

            fn module(&self) -> Option<&String> {
                self.module.as_ref()
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
        let module_field = syn::Field::parse_named
            .parse2(quote! {
                pub module: Option<String>
            })
            .unwrap();

        fields.named.push(headers_field);
        fields.named.push(request_id_field);
        fields.named.push(module_field);
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

    quote! {
        #item_struct
        #grpc_message_body
        #grpc_message_request
    }
}

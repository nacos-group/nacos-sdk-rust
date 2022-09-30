use darling::FromDeriveInput;
use quote::quote;
use syn::DeriveInput;

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(message_attr))]
struct MessageAttr {
    request_type: String,
}

#[proc_macro_derive(GrpcMessageBody, attributes(message_attr))]
pub fn grpc_message_body_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    derive(input).into()
}

fn derive(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let derive_input: DeriveInput = syn::parse2(input).unwrap();

    let message_attr = MessageAttr::from_derive_input(&derive_input).unwrap();
    let MessageAttr { request_type } = message_attr;

    let name = derive_input.ident;

    quote! {
        impl GrpcMessageBody for #name {
            fn type_url<'a>() -> std::borrow::Cow<'a, str> {
                #request_type.into()
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse() {
        let input = quote! {
            #[derive(GrpcMessageBody, Serialize, Deserialize, Debug, DeserializeOwned, Clone)]
            #[message_attr(request_type = "queryInstance")]
            struct FooSpec { foo: String }
        };

        let derive_input: DeriveInput = syn::parse2(input).unwrap();
        let message_attr = MessageAttr::from_derive_input(&derive_input).unwrap();

        println!("request type: {:?}", message_attr.request_type)
    }
}

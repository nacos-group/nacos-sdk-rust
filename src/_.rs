#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Metadata {
    #[prost(string, tag="3")]
    pub r#type: ::prost::alloc::string::String,
    #[prost(string, tag="8")]
    pub client_ip: ::prost::alloc::string::String,
    #[prost(map="string, string", tag="7")]
    pub headers: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Payload {
    #[prost(message, optional, tag="2")]
    pub metadata: ::core::option::Option<Metadata>,
    #[prost(message, optional, tag="3")]
    pub body: ::core::option::Option<::prost_types::Any>,
}
const METHOD_REQUEST_REQUEST: ::grpcio::Method<Payload, Payload> = ::grpcio::Method{ty: ::grpcio::MethodType::Unary, name: "/Request/request", req_mar: ::grpcio::Marshaller { ser: ::grpcio::pr_ser, de: ::grpcio::pr_de }, resp_mar: ::grpcio::Marshaller { ser: ::grpcio::pr_ser, de: ::grpcio::pr_de }, };
#[derive(Clone)]
pub struct RequestClient { client: ::grpcio::Client }
impl RequestClient {
pub fn new(channel: ::grpcio::Channel) -> Self { RequestClient { client: ::grpcio::Client::new(channel) }}
pub fn request_opt(&self, req: &Payload, opt: ::grpcio::CallOption) -> ::grpcio::Result<Payload,> { self.client.unary_call(&METHOD_REQUEST_REQUEST, req, opt) }
pub fn request(&self, req: &Payload) -> ::grpcio::Result<Payload,> { self.request_opt(req, ::grpcio::CallOption::default()) }
pub fn request_async_opt(&self, req: &Payload, opt: ::grpcio::CallOption) -> ::grpcio::Result<::grpcio::ClientUnaryReceiver<Payload>,> { self.client.unary_call_async(&METHOD_REQUEST_REQUEST, req, opt) }
pub fn request_async(&self, req: &Payload) -> ::grpcio::Result<::grpcio::ClientUnaryReceiver<Payload>,> { self.request_async_opt(req, ::grpcio::CallOption::default()) }
pub fn spawn<F>(&self, f: F) where F: ::std::future::Future<Output = ()> + Send + 'static {self.client.spawn(f)}
}
pub trait Request {
fn request(&mut self, ctx: ::grpcio::RpcContext, _req: Payload, sink: ::grpcio::UnarySink<Payload>) { grpcio::unimplemented_call!(ctx, sink) }
}
pub fn create_request<S: Request + Send + Clone + 'static>(s: S) -> ::grpcio::Service {
let mut builder = ::grpcio::ServiceBuilder::new();
let mut instance = s;
builder = builder.add_unary_handler(&METHOD_REQUEST_REQUEST, move |ctx, req, resp| instance.request(ctx, req, resp));
builder.build()
}
const METHOD_BI_REQUEST_STREAM_REQUEST_BI_STREAM: ::grpcio::Method<Payload, Payload> = ::grpcio::Method{ty: ::grpcio::MethodType::Duplex, name: "/BiRequestStream/requestBiStream", req_mar: ::grpcio::Marshaller { ser: ::grpcio::pr_ser, de: ::grpcio::pr_de }, resp_mar: ::grpcio::Marshaller { ser: ::grpcio::pr_ser, de: ::grpcio::pr_de }, };
#[derive(Clone)]
pub struct BiRequestStreamClient { client: ::grpcio::Client }
impl BiRequestStreamClient {
pub fn new(channel: ::grpcio::Channel) -> Self { BiRequestStreamClient { client: ::grpcio::Client::new(channel) }}
pub fn request_bi_stream_opt(&self, opt: ::grpcio::CallOption) -> ::grpcio::Result<(::grpcio::ClientDuplexSender<Payload>,::grpcio::ClientDuplexReceiver<Payload>,)> { self.client.duplex_streaming(&METHOD_BI_REQUEST_STREAM_REQUEST_BI_STREAM, opt) }
pub fn request_bi_stream(&self) -> ::grpcio::Result<(::grpcio::ClientDuplexSender<Payload>,::grpcio::ClientDuplexReceiver<Payload>,)> { self.request_bi_stream_opt(::grpcio::CallOption::default()) }
pub fn spawn<F>(&self, f: F) where F: ::std::future::Future<Output = ()> + Send + 'static {self.client.spawn(f)}
}
pub trait BiRequestStream {
fn request_bi_stream(&mut self, ctx: ::grpcio::RpcContext, _stream: ::grpcio::RequestStream<Payload>, sink: ::grpcio::DuplexSink<Payload>) { grpcio::unimplemented_call!(ctx, sink) }
}
pub fn create_bi_request_stream<S: BiRequestStream + Send + Clone + 'static>(s: S) -> ::grpcio::Service {
let mut builder = ::grpcio::ServiceBuilder::new();
let mut instance = s;
builder = builder.add_duplex_streaming_handler(&METHOD_BI_REQUEST_STREAM_REQUEST_BI_STREAM, move |ctx, req, resp| instance.request_bi_stream(ctx, req, resp));
builder.build()
}

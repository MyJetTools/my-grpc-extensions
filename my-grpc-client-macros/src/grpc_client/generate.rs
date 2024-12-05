

use std::str::FromStr;

use proc_macro::TokenStream;
use types_reader::TokensObject;


use crate::grpc_client::{fn_override::FnOverride, proto_file_reader::into_snake_case};

use super::proto_file_reader::ProtoServiceDescription;

pub fn generate(
    attr: TokenStream,
    input: TokenStream,
    with_telemetry: bool,
    with_ssh: bool,
) -> Result<proc_macro::TokenStream, syn::Error> {

    let ast: syn::DeriveInput = syn::parse(input).unwrap();

    let struct_name = &ast.ident;



    let attr: proc_macro2::TokenStream = attr.into();

    
    let params_list = TokensObject::new(attr.into())?;



    let proto_file:String = params_list.get_named_param("proto_file")?.try_into()?;


    let proto_file = ProtoServiceDescription::read_proto_file(&proto_file);

    let grpc_service_name = &proto_file.service_name;
    let grpc_service_name_token = proto_file.get_service_name_as_token();

    let interfaces = super::generate_interfaces_implementations(struct_name, &proto_file);


    let overrides = FnOverride::new(&params_list)?;

    let ping_timeout_sec:u64 = params_list.get_named_param("ping_timeout_sec")?.try_into()?;
    let ping_interval_sec:u64 = params_list.get_named_param("ping_interval_sec")?.try_into()?;
    let timeout_sec:u64 = params_list.get_named_param("request_timeout_sec")?.try_into()?;
    let retries:usize = params_list.get_named_param("retries")?.try_into()?;
    

    let crate_ns:String = params_list.get_named_param("crate_ns")?.try_into()?;
    let mut use_name_spaces = Vec::new();
    use_name_spaces.push(proc_macro2::TokenStream::from_str(format!("use {}::*", crate_ns).as_str()).unwrap());

    let ns_of_client = format!("use {}::{}::{}", crate_ns,into_snake_case(&grpc_service_name), grpc_service_name);
    use_name_spaces.push(proc_macro2::TokenStream::from_str(ns_of_client.as_str()).unwrap());


    let settings_service_name = if let Some(service_name) =  params_list.try_get_named_param("service_name"){
        service_name.unwrap_as_value()?.as_string()?.to_string()
    }else{
        struct_name.to_string()
    };

    for (override_fn_name, fn_override) in &overrides{
        if !proto_file.has_method(override_fn_name){
            let message = format!("Method {} is not found in proto file for service {}", override_fn_name, grpc_service_name);
            return Err(fn_override.token_stream.throw_error_at_value_token(message.as_str()));
        }
    }
    
    let grpc_methods = super::generate_grpc_methods(&proto_file, retries, &overrides, with_telemetry);


    let fn_create_service = if with_telemetry{
        quote::quote!{
            fn create_service(&self, channel: tonic::transport::Channel, ctx: &my_telemetry::MyTelemetryContext) -> TGrpcService {
                #grpc_service_name_token::with_interceptor(
                  channel,
                  my_grpc_extensions::GrpcClientInterceptor::new(ctx.clone()),
                )
            }
        }
    }else{
        quote::quote!{
          fn create_service(&self, channel: tonic::transport::Channel) -> TGrpcService {
             #grpc_service_name_token::new(channel)
             }
        }
    };

    let t_grpc_service = if with_telemetry{
        quote::quote!(#grpc_service_name_token<tonic::codegen::InterceptedService<tonic::transport::Channel, my_grpc_extensions::GrpcClientInterceptor>>)
    }else{
        quote::quote!(#grpc_service_name_token<tonic::transport::Channel>)
    };


    let (ssh_impl, ssh_trait) = if with_ssh{
        let ssh_impl = quote::quote!{
            pub async fn set_ssh_private_key(
                &self,
                private_key: String,
                pass_phrase: Option<String>
            ) {
                self.channel
                    .ssh_target
                    .set_private_key(private_key, pass_phrase)
                    .await;
            }
        };

        let ssh_trait = quote::quote!{
            #[async_trait::async_trait]
            impl my_grpc_extensions::GrpcClientSsh for #struct_name {
                async fn set_ssh_private_key(&self, private_key: String, pass_phrase: Option<String>){
                    self.channel
                    .ssh_target
                    .set_private_key(private_key, pass_phrase)
                    .await;
                }
            }
        };


        (ssh_impl, ssh_trait)
    }else{
        (quote::quote!(), quote::quote!())
    };

    Ok(quote::quote! {

        #(#use_name_spaces;)*

        type TGrpcService = #t_grpc_service;

        struct MyGrpcServiceFactory;

        #[async_trait::async_trait]
        impl my_grpc_extensions::GrpcServiceFactory<TGrpcService> for MyGrpcServiceFactory {
         #fn_create_service

        fn get_service_name(&self) -> &'static str {
            #struct_name::get_service_name()
        }

        async fn ping(&self, mut service: TGrpcService) {
           service.ping(()).await.unwrap();
        }
      }

      pub struct #struct_name{
        channel: my_grpc_extensions::GrpcChannelPool<TGrpcService>,
      }

      impl #struct_name{
        pub fn new(get_grpc_address: std::sync::Arc<dyn my_grpc_extensions::GrpcClientSettings + Send + Sync + 'static>,) -> Self {
            Self {
                channel: my_grpc_extensions::GrpcChannelPool::new(
                    get_grpc_address,
                    std::sync::Arc::new(MyGrpcServiceFactory),
                    std::time::Duration::from_secs(#timeout_sec),
                    std::time::Duration::from_secs(#ping_timeout_sec),
                    std::time::Duration::from_secs(#ping_interval_sec),
                    
                ),
            }
        }

        pub fn get_service_name() -> &'static str {
            #settings_service_name
        }

        #ssh_impl

        #(#grpc_methods)*  
      }

      #(#interfaces)*  

      #ssh_trait
    }
    .into())
}




# my-grpc-server-macros


### Example of usage

```rust
    #[with_telemetry]
    async fn get(
        &self,
        request: tonic::Request<GetDocumentsRequest>,
    ) -> Result<tonic::Response<Self::GetStream>, tonic::Status> {
        let request = request.into_inner();

        let result =
            crate::flows::get_docs(&self.app, request.client_id, request.doc_ids, my_telemetry)
                .await;

        return my_grpc_extensions::grpc_server::send_vec_to_stream(result, |dto| dto).await;
    }
```


#IMPORTANT

* Code must have request parameter with the name 'request'
* Code must have a line **let request = request.into_inner()**


Before the line **let request = request.into_inner()** - is going to be injected the code

```rust
let my_telemetry = my_grpc_extensions::get_telemetry(
            &request.metadata(),
            request.remote_addr(),
            #fn_name,
        );

let my_telemetry = my_telemetry.get_ctx();
```
  

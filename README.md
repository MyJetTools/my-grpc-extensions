# my-grpc-extensions

Example of usage


```rust
my-grpc-extensions = { tag = "x.x.x", git = "https://github.com/MyJetTools/my-grpc-extensions.git", features = [
    "grpc-client",
    "with-telemetry",
] }

```


Supported features
* grpc-client - gives ability to use client macros;
* grpc-server - gives ability to use server macros;
* with-telemetry - gives ability to work with telemetry;



### Connecting to GRPC server via SSH


Plug features:
* with-unix-socket
* with-ssh

Plugging to SSH leverages on the library: 

And you can use it: https://github.com/MyJetTools/my-ssh

Why unix sockets Are needed - port forward through SSH connection


#### Cargo.toml
```toml
my-grpc-extensions = { tag = "get_tag_from_github", git = "https://github.com/MyJetTools/my-grpc-extensions.git", features = [
    "grpc-client",
    "with-unix-socket",
    "with-ssh",
] }

```

#### main.rs
```rust

    // Here we specify settings behind SSH connection. For instance: http://10.0.0.1:5051 
    //  which means we connecting to remote network 10.0.0.0/24 behind SSH connection.
    let grpc_settings = GrpcLogSettings::new(over_ssh_connection.remote_resource_string);

    let grpc_client = MyLoggerGrpcClient::new(Arc::new(grpc_settings));


    //part of my_ssh library
    let ssh_credentials = my_grpc_extensions::my_ssh::SshCredentials::SshAgent{
        ssh_remote_host: "ssh_host".to_string(),
        ssh_remote_port: 22,
        ssh_user_name: "user".to_string(),
    };

  //part of my_ssh library. 
    let ssh_sessions_pool:Arc<_> = Arc::new(SshSessionsPool::new()).into();


   //Enabling SSH connection
    grpc_client.set_ssh_credentials(Arc::new(ssh_credentials)).await;

    // If we plug the pool - connection is not going to be closed after each request;
    grpc_client
            .set_ssh_sessions_pool(ssh_sessions_pool.clone())
            .await;

```


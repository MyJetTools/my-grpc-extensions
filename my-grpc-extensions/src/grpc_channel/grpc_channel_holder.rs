use std::net::SocketAddr;
use std::time::Duration;

use my_logger::LogEventCtx;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::GrpcReadError;

use super::GrpcConnectUrl;

pub struct ChannelData {
    pub channel: Channel,
    pub host: String,
    pub service_name: &'static str,
}

pub struct GrpcChannelHolder {
    pub channel: Mutex<Option<ChannelData>>,
}

impl GrpcChannelHolder {
    pub fn new() -> Self {
        Self {
            channel: Mutex::new(None),
        }
    }

    async fn set(&self, service_name: &'static str, host: String, channel: Channel) {
        let mut channel_access = self.channel.lock().await;
        *channel_access = Some(ChannelData {
            channel,
            host,
            service_name,
        });
    }

    pub async fn get(&self) -> Option<Channel> {
        let channel_access = self.channel.lock().await;

        let channel = channel_access.as_ref()?;
        Some(channel.channel.clone())
    }
    pub async fn drop_channel(&self, err: String) {
        let disconnected_channel = {
            let mut channel_access = self.channel.lock().await;
            channel_access.take()
        };

        if let Some(disconnected_channel) = disconnected_channel {
            my_logger::LOGGER.write_warning(
                "GrpcChannel::ping_channel",
                err,
                LogEventCtx::new()
                    .add("GrpcClient", disconnected_channel.service_name)
                    .add("Host", disconnected_channel.host.as_str()),
            );
        }
    }

    #[cfg(unix)]
    async fn create_unix_socket_channel(
        unix_socket_path: String,
        service_name: &'static str,
    ) -> Result<Channel, GrpcReadError> {
        use std::str::FromStr;

        use hyper::Uri;

        let uri = Uri::from_str(format!("http://unix.socket{}", unix_socket_path).as_str());

        let uri = match uri {
            Ok(uri) => uri,
            Err(err) => {
                return Err(GrpcReadError::Other(format!(
                    "Failed to create unix socket uri with path:{} for service {}. Err: {:?}",
                    unix_socket_path, service_name, err
                )));
            }
        };

        let channel = Channel::builder(uri)
            .connect_with_connector(tower::service_fn(|uri: Uri| async move {
                let unix_socket_path = uri.path();
                println!("Grpc Client connecting to {}", unix_socket_path);
                let unix_stream = tokio::net::UnixStream::connect(unix_socket_path).await?;
                // Connect to a Uds socket
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(unix_stream))
            }))
            .await;

        match channel {
            Ok(channel) => return Ok(channel),
            Err(err) => {
                println!("Can not create channel: {}", err);
                Err(err.into())
            }
        }
    }

    #[cfg(unix)]
    async fn connect_to_unix_socket(
        &self,
        connect_url: String,
        service_name: &'static str,
        request_timeout: Duration,
    ) -> Result<Channel, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let feature = Self::create_unix_socket_channel(connect_url.to_string(), service_name);

            match tokio::time::timeout(request_timeout, feature).await {
                Ok(result) => match result {
                    Ok(channel) => {
                        {
                            self.set(service_name, connect_url.to_string(), channel.clone())
                                .await;

                            my_logger::LOGGER.write_info(
                                "connect_to_unix_socket",
                                "GRPC Connection is established",
                                LogEventCtx::new()
                                    .add("GrpcClient", service_name)
                                    .add("Host".to_string(), connect_url.as_str()),
                            );
                        }
                        return Ok(channel);
                    }
                    Err(err) => {
                        if attempt_no > 3 {
                            return Err(err.into());
                        }
                    }
                },
                Err(_) => {
                    if attempt_no > 3 {
                        return Err(GrpcReadError::Timeout);
                    }
                }
            }

            attempt_no += 1;
        }
    }

    /// The port the socket goes to when the url pinned an ip: the one written in
    /// the url, or the default of its scheme.
    fn get_pinned_port(connect_url: &GrpcConnectUrl) -> Result<u16, GrpcReadError> {
        let end_point = rust_extensions::remote_endpoint::RemoteEndpoint::try_parse(
            connect_url.get_grpc_host(),
        )
        .map_err(|err| {
            GrpcReadError::Other(format!(
                "Failed to parse url:{}. Err: {}",
                connect_url.as_str(),
                err
            ))
        })?;

        match end_point.get_port() {
            Some(port) => Ok(port),
            None => Err(GrpcReadError::Other(format!(
                "Url:{} pins an ip but names no port, and its scheme has no default one",
                connect_url.as_str()
            ))),
        }
    }

    /// Opens the channel. When the url pinned an ip (`server-name@ip`) the socket
    /// goes straight to that address, while the endpoint tonic builds its requests
    /// from keeps the server name - so h2's `:authority` names the server rather
    /// than the ip, which is what a reverse proxy listening on that ip routes the
    /// call by.
    async fn connect_endpoint(
        end_point: tonic::transport::Endpoint,
        pinned_addr: Option<SocketAddr>,
    ) -> Result<Channel, tonic::transport::Error> {
        let Some(pinned_addr) = pinned_addr else {
            return end_point.connect().await;
        };

        end_point
            .connect_with_connector(tower::service_fn(move |_: hyper::Uri| async move {
                let tcp_stream = tokio::net::TcpStream::connect(pinned_addr).await?;
                // tonic's own connector does this; a custom one has to do it itself,
                // and grpc is exactly the small-message traffic Nagle holds back.
                tcp_stream.set_nodelay(true)?;
                Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(tcp_stream))
            }))
            .await
    }

    pub async fn create_channel(
        &self,
        connect_url: impl Into<GrpcConnectUrl>,
        service_name: &'static str,
        request_timeout: Duration,
        #[cfg(all(unix, feature = "with-ssh"))] ssh_target: crate::ssh::SshTargetInner,
    ) -> Result<Channel, GrpcReadError> {
        let connect_url: GrpcConnectUrl = connect_url.into();

        #[cfg(unix)]
        if connect_url.is_unix_socket() {
            return self
                .connect_to_unix_socket(
                    connect_url.get_grpc_host().to_string(),
                    service_name,
                    request_timeout,
                )
                .await;
        }

        #[cfg(all(unix, feature = "with-ssh"))]
        if let Some(ssh_credentials) = connect_url.get_ssh_credentials() {
            // A url that pinned an ip (`server-name@ip`) names the address the
            // tunnel has to reach, so the ssh server is never asked to resolve the
            // server name. Only the host and the port are read back out of this
            // string, so the scheme it gets rebuilt with does not matter.
            let forward_to = match connect_url.get_resolved_ip() {
                Ok(Some(ip)) => std::borrow::Cow::Owned(format!(
                    "http://{}",
                    SocketAddr::new(ip, Self::get_pinned_port(&connect_url)?)
                )),
                Ok(None) => std::borrow::Cow::Borrowed(connect_url.get_grpc_host()),
                Err(err) => return Err(GrpcReadError::Other(err.to_string())),
            };

            let grpc_service_endpoint =
                rust_extensions::remote_endpoint::RemoteEndpoint::try_parse(forward_to.as_ref());

            let grpc_service_endpoint = match grpc_service_endpoint {
                Ok(grpc_service_endpoint) => grpc_service_endpoint,
                Err(_) => {
                    return Err(GrpcReadError::Other(format!(
                        "Failed to parse grpc service endpoint: {} for service {}",
                        connect_url.as_str(),
                        service_name
                    )));
                }
            };

            let unix_socket_name =
                crate::ssh::generate_unix_socket_file(ssh_credentials, grpc_service_endpoint);

            let ssh_session = ssh_target.get_ssh_session(ssh_credentials).await;

            super::PORT_FORWARDS_POOL
                .start_port_forward(
                    ssh_session,
                    unix_socket_name.as_str(),
                    grpc_service_endpoint,
                )
                .await?;

            return self
                .connect_to_unix_socket(unix_socket_name, service_name, request_timeout)
                .await;
        }

        let pinned_addr = match connect_url.get_resolved_ip() {
            Ok(Some(ip)) => Some(SocketAddr::new(ip, Self::get_pinned_port(&connect_url)?)),
            Ok(None) => None,
            Err(err) => return Err(GrpcReadError::Other(err.to_string())),
        };

        let mut attempt_no = 0;
        loop {
            let end_point = match Channel::from_shared(connect_url.get_grpc_host().to_string()) {
                Ok(end_point) => end_point,
                Err(err) => {
                    return Err(GrpcReadError::Other(format!(
                        "Failed to create channel with url:{}. Err: {:?}",
                        connect_url.as_str(),
                        err
                    )));
                }
            };

            #[cfg(any(feature = "with-ring-tls", feature = "with-rust-tls"))]
            if connect_url.is_grpc_tls_endpoint() {
                //let cert = Certificate::from_pem(my_tls::ALL_CERTIFICATES);
                // let tls = ClientTlsConfig::new()
                //    .ca_certificate(cert)
                //    .domain_name(super::extract_domain_name(connect_url.as_str()));
                // end_point = end_point.tls_config(tls).unwrap();
                return Err(GrpcReadError::Other(format!(
                    "TLS is not implemented yet for endpoint: {}",
                    connect_url.as_str()
                )));
            }

            match tokio::time::timeout(
                request_timeout,
                Self::connect_endpoint(end_point, pinned_addr),
            )
            .await
            {
                Ok(channel) => match channel {
                    Ok(channel) => {
                        {
                            self.set(
                                service_name,
                                connect_url.get_grpc_host().to_string(),
                                channel.clone(),
                            )
                            .await;

                            my_logger::LOGGER.write_info(
                                "create_channel",
                                "GRPC Connection is established",
                                LogEventCtx::new()
                                    .add("GrpcClient", service_name)
                                    .add("Host".to_string(), connect_url.as_str()),
                            );
                        }
                        return Ok(channel);
                    }
                    Err(err) => {
                        if attempt_no > 3 {
                            return Err(err.into());
                        }
                    }
                },
                Err(_) => {
                    if attempt_no > 3 {
                        return Err(GrpcReadError::Timeout);
                    }
                }
            }

            attempt_no += 1;
        }
    }
}

#[cfg(test)]
mod test {

    #[test]
    fn test_unix_socket_pass_to_uri() {
        use hyper::Uri;

        let uri = Uri::builder()
            .scheme("http")
            .path_and_query("/tmp/test.sock")
            .authority("unix.socket")
            .build()
            .unwrap();

        assert_eq!("/tmp/test.sock", uri.path_and_query().unwrap().as_str());
    }

    /// The whole point of `server-name@ip`: the socket goes to the ip, but the
    /// request that comes out of it names the server. my-reverse-proxy picks an
    /// endpoint by `req.uri().host()` - h2's `:authority` - so the server name is
    /// what has to arrive there, not the ip the connection went to.
    #[tokio::test]
    async fn a_pinned_ip_dials_the_ip_and_sends_the_server_name_as_authority() {
        use hyper::service::service_fn;
        use hyper_util::rt::{TokioExecutor, TokioIo};
        use std::convert::Infallible;
        use std::time::Duration;
        use tower::ServiceExt;

        // Nothing resolves `my-service.example.com`; the listener is on 127.0.0.1,
        // so the call can only arrive by the pinned ip.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (tx, rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // `service_fn` wants an `Fn`, and the sender is single-use.
            let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

            let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                        let tx = tx.clone();
                        if let Some(tx) = tx.lock().unwrap().take() {
                            let _ = tx.send(req.uri().host().map(|host| host.to_string()));
                        }
                        async {
                            Ok::<_, Infallible>(hyper::Response::new(
                                tonic::body::Body::empty(),
                            ))
                        }
                    }),
                )
                .await;
        });

        let channel = super::GrpcChannelHolder::new()
            .create_channel(
                format!("http://my-service.example.com@127.0.0.1:{}", port),
                "test",
                Duration::from_secs(5),
                #[cfg(all(unix, feature = "with-ssh"))]
                crate::ssh::SshTargetInner {
                    private_key_resolver: None,
                },
            )
            .await
            .unwrap();

        let request = hyper::Request::builder()
            .method("POST")
            .uri("/test.Service/Method")
            .body(tonic::body::Body::empty())
            .unwrap();

        let _ = channel.oneshot(request).await;

        let authority = tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .expect("the server never saw the request")
            .unwrap();

        assert_eq!(authority.as_deref(), Some("my-service.example.com"));
    }
}

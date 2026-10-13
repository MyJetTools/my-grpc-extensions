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

    /// `origin` is what the requests name in h2's `:scheme` and `:authority`.
    /// Without one they name `unix.socket` - the host of the uri the channel is
    /// built from, which only carries the socket path to the connector.
    #[cfg(unix)]
    async fn create_unix_socket_channel(
        unix_socket_path: String,
        origin: Option<hyper::Uri>,
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

        let mut end_point = Channel::builder(uri);

        if let Some(origin) = origin {
            end_point = end_point.origin(origin);
        }

        let channel = end_point
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
        origin: Option<hyper::Uri>,
        service_name: &'static str,
        request_timeout: Duration,
    ) -> Result<Channel, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let feature = Self::create_unix_socket_channel(
                connect_url.to_string(),
                origin.clone(),
                service_name,
            );

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

    /// What a call made through an ssh tunnel names in h2's `:authority`: the host
    /// of the url behind the tunnel - the server name, when that url pinned an ip.
    /// The channel itself is opened to a local unix socket, so without it every
    /// request would name `unix.socket` and a reverse proxy on the far side of the
    /// tunnel would have nothing to route the call by. The scheme is always `http`:
    /// that is what goes through the tunnel.
    #[cfg(all(unix, feature = "with-ssh"))]
    fn get_tunnel_origin(connect_url: &GrpcConnectUrl) -> Result<hyper::Uri, GrpcReadError> {
        use std::str::FromStr;

        let uri = hyper::Uri::from_str(connect_url.get_grpc_host()).map_err(|err| {
            GrpcReadError::Other(format!(
                "Failed to parse url:{}. Err: {}",
                connect_url.as_str(),
                err
            ))
        })?;

        let Some(authority) = uri.authority() else {
            return Err(GrpcReadError::Other(format!(
                "Url:{} names no host behind the ssh tunnel",
                connect_url.as_str()
            )));
        };

        hyper::Uri::builder()
            .scheme("http")
            .authority(authority.clone())
            .path_and_query("/")
            .build()
            .map_err(|err| {
                GrpcReadError::Other(format!(
                    "Failed to parse url:{}. Err: {}",
                    connect_url.as_str(),
                    err
                ))
            })
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
                    None,
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

            let origin = Self::get_tunnel_origin(&connect_url)?;

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
                .connect_to_unix_socket(
                    unix_socket_name,
                    Some(origin),
                    service_name,
                    request_timeout,
                )
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
        use std::time::Duration;

        // Nothing resolves `my-service.example.com`; the listener is on 127.0.0.1,
        // so the call can only arrive by the pinned ip.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();

        let (tx, rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            serve_and_report_host(stream, tx).await;
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

        assert_eq!(
            get_reported_host(channel, rx).await.as_deref(),
            Some("my-service.example.com")
        );
    }

    /// A channel to a unix socket names `unix.socket`, unless it is given an
    /// origin - which is how a call through an ssh tunnel, opened to a local
    /// socket, still names the server behind the tunnel.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_unix_socket_channel_names_the_origin_it_was_given() {
        for (no, (origin, expected_host)) in [
            (None, "unix.socket"),
            (
                Some("http://my-service.example.com:8080"),
                "my-service.example.com",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            // Short on purpose: a unix socket path has to fit into ~100 bytes.
            let socket_path =
                std::env::temp_dir().join(format!("mge-{}-{}.sock", std::process::id(), no));
            let _ = std::fs::remove_file(&socket_path);
            let listener = tokio::net::UnixListener::bind(&socket_path).unwrap();

            let (tx, rx) = tokio::sync::oneshot::channel();

            tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                serve_and_report_host(stream, tx).await;
            });

            let channel = super::GrpcChannelHolder::create_unix_socket_channel(
                socket_path.to_str().unwrap().to_string(),
                origin.map(|origin: &str| origin.parse().unwrap()),
                "test",
            )
            .await
            .unwrap();

            assert_eq!(
                get_reported_host(channel, rx).await.as_deref(),
                Some(expected_host)
            );

            let _ = std::fs::remove_file(&socket_path);
        }
    }

    #[cfg(all(unix, feature = "with-ssh"))]
    #[test]
    fn a_tunnel_names_the_server_behind_it() {
        for (url, expected_authority) in [
            (
                "ssh://user@ssh-host:22->http://my-service.example.com@10.0.0.7:8080",
                "my-service.example.com:8080",
            ),
            (
                "ssh://user@ssh-host:22->http://my-service:8080",
                "my-service:8080",
            ),
            // What goes through the tunnel is plain h2, whatever the url says.
            (
                "ssh://user@ssh-host:22->https://my-service.example.com@10.0.0.7:8443",
                "my-service.example.com:8443",
            ),
        ] {
            let connect_url: super::GrpcConnectUrl = url.to_string().into();
            let origin = super::GrpcChannelHolder::get_tunnel_origin(&connect_url).unwrap();

            assert_eq!(origin.scheme_str(), Some("http"), "{url}");
            assert_eq!(
                origin.authority().map(|authority| authority.as_str()),
                Some(expected_authority),
                "{url}"
            );
        }
    }

    /// Serves one h2 connection and reports the host its first request names -
    /// h2's `:authority`.
    async fn serve_and_report_host<TStream>(
        stream: TStream,
        tx: tokio::sync::oneshot::Sender<Option<String>>,
    ) where
        TStream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        use hyper::service::service_fn;
        use hyper_util::rt::{TokioExecutor, TokioIo};
        use std::convert::Infallible;

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
                        Ok::<_, Infallible>(hyper::Response::new(tonic::body::Body::empty()))
                    }
                }),
            )
            .await;
    }

    /// Sends one request through the channel and waits for the host the server
    /// saw in it.
    async fn get_reported_host(
        channel: tonic::transport::Channel,
        rx: tokio::sync::oneshot::Receiver<Option<String>>,
    ) -> Option<String> {
        use tower::ServiceExt;

        let request = hyper::Request::builder()
            .method("POST")
            .uri("/test.Service/Method")
            .body(tonic::body::Body::empty())
            .unwrap();

        let _ = channel.oneshot(request).await;

        tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await
            .expect("the server never saw the request")
            .unwrap()
    }
}

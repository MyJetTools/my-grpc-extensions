use std::{sync::Arc, time::Duration};

use my_logger::LogEventCtx;
#[cfg(feature = "with-telemetry")]
use my_telemetry::MyTelemetryContext;

use rust_extensions::UnsafeValue;
use tokio::time::error::Elapsed;
use tonic::transport::Channel;

use crate::{GrpcChannel, GrpcChannelHolder};

#[derive(Debug)]
pub enum GrpcReadError {
    Timeout,
    TransportError(tonic::transport::Error),
    TonicStatus(tonic::Status),
}

#[async_trait::async_trait]
pub trait GrpcClientSettings {
    async fn get_grpc_url(&self, name: &'static str) -> GrpcUrl;
}

#[derive(Debug)]
pub struct GrpcUrl {
    pub url: String,
    pub host_metadata: Option<String>,
}

impl Into<GrpcUrl> for String {
    fn into(self) -> GrpcUrl {
        GrpcUrl {
            url: self,
            host_metadata: None,
        }
    }
}

#[async_trait::async_trait]
pub trait GrpcServiceFactory<TService: Send + Sync + 'static> {
    fn create_service(
        &self,
        channel: Channel,
        #[cfg(feature = "with-telemetry")] ctx: &MyTelemetryContext,
    ) -> TService;
    fn get_service_name(&self) -> &'static str;
    async fn ping(&self, service: TService);
}

pub struct GrpcChannelPool<TService: Send + Sync + 'static> {
    pub grpc_channel_holder: Arc<GrpcChannelHolder>,
    pub request_timeout: Duration,
    pub ping_timeout: Duration,
    pub ping_interval: Duration,
    get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    #[cfg(feature = "with-ssh")]
    pub ssh_target: crate::SshTarget,
    enable_ping: Arc<UnsafeValue<bool>>,
}

impl<'s, TService: Send + Sync + 'static> GrpcChannelPool<TService> {
    pub fn new(
        get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        request_timeout: Duration,
        ping_timeout: Duration,
        ping_interval: Duration,
    ) -> Self {
        let result = Self {
            grpc_channel_holder: Arc::new(GrpcChannelHolder::new()),
            request_timeout,
            ping_timeout,
            ping_interval,
            get_grpc_address,
            service_factory,
            #[cfg(feature = "with-ssh")]
            ssh_target: crate::SshTarget::new(),
            enable_ping: Arc::new(UnsafeValue::new(false)),
        };

        result.ping_channel();

        result
    }

    pub fn get_channel(
        &self,
        #[cfg(feature = "with-telemetry")] ctx: &MyTelemetryContext,
    ) -> GrpcChannel<TService> {
        self.enable_ping.set_value(true);
        return GrpcChannel::new(
            self.grpc_channel_holder.clone(),
            self.request_timeout,
            self.service_factory.clone(),
            self.get_grpc_address.clone(),
            #[cfg(feature = "with-telemetry")]
            ctx.clone(),
            #[cfg(feature = "with-ssh")]
            self.ssh_target.clone(),
        );
    }

    fn ping_channel(&self) {
        #[cfg(feature = "with-ssh")]
        let ssh_target = self.ssh_target.clone();
        let enable_ping = self.enable_ping.clone();
        let ping_interval = self.ping_interval;
        let ping_timeout = self.ping_timeout;
        let grpc_channel_holder = self.grpc_channel_holder.clone();
        let grpc_client_settings = self.get_grpc_address.clone();
        let grpc_service_factory = self.service_factory.clone();
        let request_timeout = self.request_timeout;
        tokio::spawn(ping_loop(
            #[cfg(feature = "with-ssh")]
            ssh_target,
            enable_ping,
            ping_interval,
            ping_timeout,
            request_timeout,
            grpc_channel_holder,
            grpc_client_settings,
            grpc_service_factory,
        ));
    }
}

async fn get_or_create_channel<TService: Send + Sync + 'static>(
    #[cfg(feature = "with-ssh")] ssh_target: &crate::SshTarget,
    grpc_channel_holder: &Arc<GrpcChannelHolder>,
    grpc_client_settings: &Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    grpc_service_factory: &Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    request_timeout: Duration,
) -> Result<Channel, GrpcReadError> {
    match grpc_channel_holder.get().await {
        Some(channel) => Ok(channel),
        None => {
            let grpc_url = grpc_client_settings
                .get_grpc_url(grpc_service_factory.get_service_name())
                .await;
            my_logger::LOGGER.write_warning(
                "GrpcChannel::ping_channel",
                "Channel is not available. Creating One",
                LogEventCtx::new()
                    .add("GrpcClient", grpc_service_factory.get_service_name())
                    .add("Host", grpc_url.url.to_string()),
            );

            grpc_channel_holder
                .create_channel(
                    grpc_url.url,
                    grpc_service_factory.get_service_name(),
                    request_timeout,
                    #[cfg(feature = "with-ssh")]
                    ssh_target.get_value().await,
                )
                .await
        }
    }
}

async fn ping_loop<TService: Send + Sync + 'static>(
    #[cfg(feature = "with-ssh")] ssh_target: crate::SshTarget,
    enable_ping: Arc<UnsafeValue<bool>>,
    ping_interval: Duration,
    ping_timeout: Duration,
    request_timeout: Duration,
    grpc_channel_holder: Arc<GrpcChannelHolder>,
    grpc_client_settings: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    grpc_service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
) {
    loop {
        if !enable_ping.get_value() {
            tokio::time::sleep(ping_interval).await;
            continue;
        }

        let channel = get_or_create_channel(
            #[cfg(feature = "with-ssh")]
            &ssh_target,
            &grpc_channel_holder,
            &grpc_client_settings,
            &grpc_service_factory,
            request_timeout,
        )
        .await;

        match channel {
            Ok(channel) => {
                let service = grpc_service_factory.create_service(
                    channel,
                    #[cfg(feature = "with-telemetry")]
                    &MyTelemetryContext::create_empty(),
                );

                let result = tokio::spawn(execute_ping_with_timeout(
                    grpc_service_factory.clone(),
                    ping_timeout,
                    service,
                ))
                .await;

                match result {
                    Ok(result) => match result {
                        PingResult::Ok => {}
                        PingResult::Timeout => {
                            grpc_channel_holder
                                .drop_channel("Ping Timeout".to_string())
                                .await;
                        }
                    },
                    Err(_) => {
                        grpc_channel_holder
                            .drop_channel("Ping Panic".to_string())
                            .await;
                    }
                };
            }
            Err(err) => {
                let mut grpc_url = grpc_client_settings
                    .get_grpc_url(grpc_service_factory.get_service_name())
                    .await;

                let mut ctx = LogEventCtx::new()
                    .add("GrpcClient", grpc_service_factory.get_service_name())
                    .add("Url", grpc_url.url);

                if let Some(host) = grpc_url.host_metadata.take() {
                    ctx = ctx.add("HostMetadatas", host);
                }

                my_logger::LOGGER.write_error(
                    "GrpcChannel::ping_channel",
                    format!("{:?}", err),
                    ctx,
                );
            }
        }
        tokio::time::sleep(ping_interval).await;
    }
}

async fn execute_ping_with_timeout<TService: Send + Sync + 'static>(
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
    ping_timeout: Duration,
    service: TService,
) -> PingResult {
    let future = service_factory.ping(service);

    if tokio::time::timeout(ping_timeout, future).await.is_err() {
        return PingResult::Timeout;
    }

    PingResult::Ok
}

pub enum PingResult {
    Ok,
    Timeout,
}

impl From<Elapsed> for GrpcReadError {
    fn from(_: Elapsed) -> Self {
        Self::Timeout
    }
}

impl From<tonic::Status> for GrpcReadError {
    fn from(value: tonic::Status) -> Self {
        Self::TonicStatus(value)
    }
}

impl From<tonic::transport::Error> for GrpcReadError {
    fn from(value: tonic::transport::Error) -> Self {
        Self::TransportError(value)
    }
}

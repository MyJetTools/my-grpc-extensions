use std::{sync::Arc, time::Duration};

use my_logger::LogEventCtx;
#[cfg(feature = "with-telemetry")]
use my_telemetry::MyTelemetryContext;

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
    async fn get_grpc_url(&self, name: &'static str) -> String;
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
    ssh_target: crate::SshTarget,
}

impl<'s, TService: Send + Sync + 'static> GrpcChannelPool<TService> {
    pub fn new(
        get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        request_timeout: Duration,
        ping_timeout: Duration,
        ping_interval: Duration,
        #[cfg(feature = "with-ssh")] ssh_target: crate::SshTarget,
    ) -> Self {
        let result = Self {
            grpc_channel_holder: Arc::new(GrpcChannelHolder::new()),
            request_timeout,
            ping_timeout,
            ping_interval,
            get_grpc_address,
            service_factory,
            #[cfg(feature = "with-ssh")]
            ssh_target,
        };

        result.ping_channel();

        result
    }

    pub fn get_channel(
        &self,
        #[cfg(feature = "with-telemetry")] ctx: &MyTelemetryContext,
    ) -> GrpcChannel<TService> {
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
        let get_grpc_address = self.get_grpc_address.clone();
        let service_name = self.service_factory.get_service_name();
        let service_factory = self.service_factory.clone();

        let inner = self.grpc_channel_holder.clone();

        let request_timeout = self.request_timeout;
        let ping_interval = self.ping_interval;
        let ping_timeout = self.ping_timeout;
        #[cfg(feature = "with-ssh")]
        let ssh_target = self.ssh_target.clone();
        tokio::spawn(async move {
            loop {
                let channel = match inner.reuse_existing_channel().await {
                    Some(channel) => Ok(channel),
                    None => {
                        let connect_url = get_grpc_address.get_grpc_url(service_name).await;
                        my_logger::LOGGER.write_warning(
                            "GrpcChannel::ping_channel",
                            "Channel is not available. Creating One",
                            LogEventCtx::new()
                                .add("GrpcClient", service_name)
                                .add("Host", connect_url.to_string()),
                        );

                        inner
                            .create_channel(
                                connect_url,
                                service_name,
                                request_timeout,
                                #[cfg(feature = "with-ssh")]
                                ssh_target.get_value().await,
                            )
                            .await
                    }
                };

                match channel {
                    Ok(channel) => {
                        let service = service_factory.create_service(
                            channel,
                            #[cfg(feature = "with-telemetry")]
                            &MyTelemetryContext::new(),
                        );

                        let service_factory_cloned = service_factory.clone();

                        let result = tokio::spawn(async move {
                            let future = service_factory_cloned.ping(service);

                            if tokio::time::timeout(ping_timeout, future).await.is_err() {
                                return PingResult::Timeout;
                            }

                            PingResult::Ok
                        })
                        .await;

                        match result {
                            Ok(result) => match result {
                                PingResult::Ok => {}
                                PingResult::Timeout => {
                                    inner.drop_channel("Ping Timeout".to_string()).await;
                                }
                            },
                            Err(_) => {
                                inner.drop_channel("Ping Panic".to_string()).await;
                            }
                        };
                    }
                    Err(err) => {
                        my_logger::LOGGER.write_error(
                            "GrpcChannel::ping_channel",
                            format!("{:?}", err),
                            LogEventCtx::new()
                                .add("GrpcClient", service_name)
                                .add("Host", get_grpc_address.get_grpc_url(service_name).await),
                        );
                    }
                }
                tokio::time::sleep(ping_interval).await;
            }
        });
    }
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

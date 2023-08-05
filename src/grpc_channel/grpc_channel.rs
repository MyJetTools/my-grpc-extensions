use std::{sync::Arc, time::Duration};

use my_logger::LogEventCtx;
use my_telemetry::MyTelemetryContext;

use tokio::{sync::Mutex, time::error::Elapsed};
use tonic::transport::Channel;

use crate::{GrpcChannelPool, RentedChannel};

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
    fn create_service(&self, channel: Channel, ctx: &MyTelemetryContext) -> TService;
    fn get_service_name(&self) -> &'static str;
    async fn ping(&self, service: TService);
}

pub struct GrpcChannel<TService: Send + Sync + 'static> {
    pub channel_pool: Arc<Mutex<GrpcChannelPool>>,
    pub request_timeout: Duration,
    pub ping_timeout: Duration,
    pub ping_interval: Duration,

    get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
}

impl<'s, TService: Send + Sync + 'static> GrpcChannel<TService> {
    pub fn new(
        get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
        service_factory: Arc<dyn GrpcServiceFactory<TService> + Send + Sync + 'static>,
        request_timeout: Duration,
        ping_timeout: Duration,
        ping_interval: Duration,
    ) -> Self {
        let channel_pool = Arc::new(Mutex::new(GrpcChannelPool::new()));
        let result = Self {
            channel_pool,
            request_timeout,
            ping_timeout,
            ping_interval,
            get_grpc_address,
            service_factory,
        };

        result.ping_channel();

        result
    }

    pub async fn get_channel(
        &self,
        ctx: &MyTelemetryContext,
    ) -> Result<RentedChannel<TService>, GrpcReadError> {
        {
            let mut access = self.channel_pool.lock().await;
            if let Some(channel) = access.rent() {
                return Ok(RentedChannel::new(
                    channel,
                    self.request_timeout,
                    self.service_factory.clone(),
                    ctx.clone(),
                ));
            }
        }

        let mut attempt_no = 0;
        loop {
            let grpc_address = self
                .get_grpc_address
                .get_grpc_url(self.service_factory.get_service_name())
                .await;
            let end_point = Channel::from_shared(grpc_address.clone());

            if let Err(err) = end_point {
                panic!(
                    "Failed to create channel with url:{}. Err: {:?}",
                    grpc_address, err
                )
            }

            let end_point = end_point.unwrap();

            match tokio::time::timeout(self.request_timeout, end_point.connect()).await {
                Ok(channel) => match channel {
                    Ok(channel) => {
                        {
                            let mut access = self.channel_pool.lock().await;
                            access.set(
                                self.service_factory.get_service_name(),
                                grpc_address,
                                channel.clone(),
                            );
                        }
                        return Ok(RentedChannel::new(
                            channel,
                            self.request_timeout,
                            self.service_factory.clone(),
                            ctx.clone(),
                        ));
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

    fn ping_channel(&self) {
        let get_grpc_address = self.get_grpc_address.clone();
        let service_name = self.service_factory.get_service_name();
        let service_factory = self.service_factory.clone();
        let channel_pool: Arc<Mutex<GrpcChannelPool>> = self.channel_pool.clone();
        let request_timeout = self.request_timeout;
        let ping_interval = self.ping_interval;
        let ping_timeout = self.ping_timeout;
        tokio::spawn(async move {
            loop {
                let channel = {
                    let mut access = channel_pool.lock().await;
                    access.rent()
                };

                let mut create_channel = false;

                if let Some(channel) = channel {
                    let service =
                        service_factory.create_service(channel, &MyTelemetryContext::new());

                    let service_factory_cloned = service_factory.clone();

                    let result = tokio::spawn(async move {
                        let future = service_factory_cloned.ping(service);

                        if tokio::time::timeout(ping_timeout, future).await.is_err() {
                            return PingResult::Timeout;
                        }

                        PingResult::Ok
                    })
                    .await;

                    let ping_error = match result {
                        Ok(result) => match result {
                            PingResult::Ok => None,
                            PingResult::Timeout => Some("Timeout".to_string()),
                        },
                        Err(err) => Some(format!("Error: {:?}", err)),
                    };

                    if let Some(err) = ping_error {
                        {
                            let mut access = channel_pool.lock().await;
                            if let Some(host) = access.disconnect_channel() {
                                my_logger::LOGGER.write_warning(
                                    "GrpcChannel::ping_channel",
                                    "Ping fail. Disconnecting channel".to_string(),
                                    LogEventCtx::new()
                                        .add("GrpcClient", service_name)
                                        .add("Host", host)
                                        .add("FailType", err),
                                );
                            }
                        }

                        create_channel = true;
                    }
                } else {
                    create_channel = true;
                }

                if create_channel {
                    let grpc_address = get_grpc_address.get_grpc_url(service_name).await;

                    let end_point = Channel::from_shared(grpc_address.clone());

                    if let Ok(end_point) = end_point {
                        if let Ok(channel) =
                            tokio::time::timeout(request_timeout, end_point.connect()).await
                        {
                            match channel {
                                Ok(channel) => {
                                    let mut access = channel_pool.lock().await;
                                    access.set(service_name, grpc_address, channel.clone());
                                }
                                Err(err) => {
                                    my_logger::LOGGER.write_error(
                                        "GrpcChannel::ping_channel",
                                        format!(
                                            "Can not connect to the channel {:?}. Err: {:?}",
                                            end_point, err
                                        ),
                                        LogEventCtx::new()
                                            .add("GrpcClient", service_name)
                                            .add("Host", grpc_address),
                                    );
                                }
                            }
                        } else {
                            my_logger::LOGGER.write_error(
                                "GrpcChannel::ping_channel",
                                "Can not connect to GrpcClient",
                                LogEventCtx::new()
                                    .add("GrpcClient", service_name)
                                    .add("Host", format!("{:?}", end_point)),
                            );
                        }
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

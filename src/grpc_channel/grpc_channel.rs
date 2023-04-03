use std::{sync::Arc, time::Duration};

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

pub struct GrpcChannel {
    pub channel_pool: Arc<Mutex<GrpcChannelPool>>,
    pub timeout: Duration,
    get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
    service_name: &'static str,
}

impl GrpcChannel {
    pub fn new(
        get_grpc_address: Arc<dyn GrpcClientSettings + Send + Sync + 'static>,
        service_name: &'static str,
        timeout: Duration,
        max_services_pool_amount: usize,
    ) -> Self {
        Self {
            channel_pool: Arc::new(Mutex::new(GrpcChannelPool::new(max_services_pool_amount))),
            timeout,
            get_grpc_address,
            service_name,
        }
    }

    pub async fn get_channel<TService>(
        &self,
        create_service: impl Fn(Channel) -> TService,
    ) -> Result<RentedChannel<TService>, GrpcReadError> {
        {
            let mut access = self.channel_pool.lock().await;
            if let Some(channel) = access.rent() {
                let service = create_service(channel.clone());
                return Ok(RentedChannel::new(
                    channel,
                    self.channel_pool.clone(),
                    self.timeout,
                    service,
                ));
            }
        }

        let mut attempt_no = 0;
        loop {
            let grpc_address = self.get_grpc_address.get_grpc_url(self.service_name).await;
            let end_point = Channel::from_shared(grpc_address.clone());

            if let Err(err) = end_point {
                panic!(
                    "Failed to create channel with url:{}. Err: {:?}",
                    grpc_address, err
                )
            }

            let end_point = end_point.unwrap();

            match tokio::time::timeout(self.timeout, end_point.connect()).await {
                Ok(channel) => match channel {
                    Ok(channel) => {
                        let service = create_service(channel.clone());
                        return Ok(RentedChannel::new(
                            channel,
                            self.channel_pool.clone(),
                            self.timeout,
                            service,
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

use std::time::Duration;

use my_logger::LogEventCtx;
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::GrpcReadError;

#[cfg(feature = "with-tls")]
use tonic::transport::{Certificate, ClientTlsConfig};

pub struct ChannelData {
    pub channel: Channel,
    pub host: String,
    pub service_name: &'static str,
}

pub struct GrpcChannelHolder {
    //todo!("Temporary there is no pool. Debugging retry/reconnect features - and then pool would be added.")
    pub channel: Mutex<Option<ChannelData>>,
}

impl GrpcChannelHolder {
    pub fn new() -> Self {
        Self {
            channel: Mutex::new(None),
        }
    }

    async fn set(&self, service_name: &'static str, host: String, channel: Channel) {
        my_logger::LOGGER.write_info(
            "GrpcChannelPoolInner::set",
            "GRPC Connection is established",
            LogEventCtx::new()
                .add("GrpcClient", service_name)
                .add("Host".to_string(), host.to_string()),
        );

        let mut channel_access = self.channel.lock().await;
        *channel_access = Some(ChannelData {
            channel,
            host,
            service_name,
        });
    }

    pub async fn reuse_existing_channel(&self) -> Option<Channel> {
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
                    .add("Host", disconnected_channel.host),
            );
        }
    }

    pub async fn create_channel(
        &self,
        connect_url: String,
        service_name: &'static str,
        request_timeout: Duration,
    ) -> Result<Channel, GrpcReadError> {
        let mut attempt_no = 0;
        loop {
            let end_point = Channel::from_shared(connect_url.clone());

            if let Err(err) = end_point {
                panic!(
                    "Failed to create channel with url:{}. Err: {:?}",
                    connect_url, err
                )
            }

            #[cfg(feature = "with-tls")]
            let mut end_point = end_point.unwrap();

            #[cfg(not(feature = "with-tls"))]
            let end_point = end_point.unwrap();

            #[cfg(feature = "with-tls")]
            if connect_url.to_lowercase().starts_with("https") {
                let cert = Certificate::from_pem(my_tls::ALL_CERTIFICATES);
                let tls = ClientTlsConfig::new()
                    .ca_certificate(cert)
                    .domain_name(super::extract_domain_name(connect_url.as_str()));
                end_point = end_point.tls_config(tls).unwrap();
            }

            match tokio::time::timeout(request_timeout, end_point.connect()).await {
                Ok(channel) => match channel {
                    Ok(channel) => {
                        {
                            self.set(service_name, connect_url, channel.clone()).await;
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

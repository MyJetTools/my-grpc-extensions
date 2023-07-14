use my_logger::LogEventCtx;
use tonic::transport::Channel;

pub struct GrpcChannelPool {
    pub channel: Option<Channel>,
}

impl GrpcChannelPool {
    pub fn new() -> Self {
        Self { channel: None }
    }

    pub fn set(&mut self, service_name: &'static str, channel: Channel) {
        self.channel = Some(channel);

        my_logger::LOGGER.write_info(
            "GrpcChannelPool::set",
            "GRPC Connection is established for service",
            LogEventCtx::new().add("GrpcClient", service_name),
        );
    }

    pub fn rent(&mut self) -> Option<Channel> {
        self.channel.clone()
    }
    pub fn disconnect_channel(&mut self) {
        self.channel = None;
    }
}

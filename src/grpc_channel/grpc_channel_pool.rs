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
            format!("GrpcService {}", service_name),
            format!("GRPC Connection established for service: {}", service_name),
            LogEventCtx::new(),
        );
    }

    pub fn rent(&mut self) -> Option<Channel> {
        self.channel.clone()
    }
    pub fn disconnect_channel(&mut self) {
        self.channel = None;
    }
}

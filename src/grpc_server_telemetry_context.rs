use std::net::SocketAddr;

use my_telemetry::{MyTelemetryContext, TelemetryEvent};
use rust_extensions::date_time::DateTimeAsMicroseconds;

pub struct GrpcServerTelemetryContext {
    pub ctx: MyTelemetryContext,
    pub stareted: DateTimeAsMicroseconds,
    addr: Option<SocketAddr>,
    method: String,
}

impl GrpcServerTelemetryContext {
    pub fn new(ctx: MyTelemetryContext, addr: Option<SocketAddr>, method: String) -> Self {
        Self {
            ctx,
            stareted: DateTimeAsMicroseconds::now(),
            addr,
            method,
        }
    }
}

impl Drop for GrpcServerTelemetryContext {
    fn drop(&mut self) {
        if !my_telemetry::TELEMETRY_INTERFACE.is_telemetry_set_up() {
            return;
        }

        let addr = if let Some(addr) = self.addr {
            addr.to_string().into()
        } else {
            None
        };

        tokio::spawn(
            my_telemetry::TELEMETRY_INTERFACE.write_telemetry_event(TelemetryEvent {
                process_id: self.ctx.process_id,
                started: self.stareted.unix_microseconds,
                finished: DateTimeAsMicroseconds::now().unix_microseconds,
                data: format!("GRPC: {}", self.method),
                success: "Done".to_lowercase().into(),
                fail: None,
                ip: addr,
            }),
        );
    }
}

pub fn get_telemetry(
    metadata: &tonic::metadata::MetadataMap,
    addr: Option<SocketAddr>,
    method: &str,
) -> GrpcServerTelemetryContext {
    if let Some(process_id) = metadata.get("process-id") {
        if let Ok(process_id) = std::str::from_utf8(process_id.as_bytes()) {
            if let Ok(process_id) = process_id.parse::<i64>() {
                return GrpcServerTelemetryContext::new(
                    MyTelemetryContext { process_id },
                    addr,
                    method.to_string(),
                );
            }
        }
    }

    return GrpcServerTelemetryContext::new(MyTelemetryContext::new(), addr, method.to_string());
}

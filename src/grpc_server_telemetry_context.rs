use std::net::SocketAddr;

use my_telemetry::MyTelemetryContext;
use rust_extensions::date_time::DateTimeAsMicroseconds;

pub struct GrpcServerTelemetryContext {
    ctx: Option<MyTelemetryContext>,
    pub started: DateTimeAsMicroseconds,
    addr: Option<SocketAddr>,
    method: Option<String>,
}

impl GrpcServerTelemetryContext {
    pub fn new(ctx: MyTelemetryContext, addr: Option<SocketAddr>, method: String) -> Self {
        Self {
            ctx: Some(ctx),
            started: DateTimeAsMicroseconds::now(),
            addr,
            method: Some(method),
        }
    }

    pub fn get_ctx(&self) -> &MyTelemetryContext {
        if self.ctx.is_none() {
            panic!("MyTelemetry context is not set");
        }
        self.ctx.as_ref().unwrap()
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

        let started = self.started;

        let method = self.method.take();

        if let Some(ctx) = self.ctx.take() {
            if let Some(method) = method {
                tokio::spawn(async move {
                    my_telemetry::TELEMETRY_INTERFACE
                        .write_success(
                            &ctx,
                            started,
                            format!("GRPC: {}", method),
                            "done".to_string(),
                            addr,
                        )
                        .await;
                });
            }
        }
    }
}

pub fn get_telemetry(
    metadata: &tonic::metadata::MetadataMap,
    addr: Option<SocketAddr>,
    method: &str,
) -> GrpcServerTelemetryContext {
    if let Some(process_id) = metadata.get("process-id") {
        if let Ok(process_id) = std::str::from_utf8(process_id.as_bytes()) {
            if has_multiple_ids(process_id.as_bytes()) {
                let mut ids = Vec::new();

                for itm in process_id.split(',') {
                    if let Ok(id) = itm.parse::<i64>() {
                        ids.push(id);
                    }
                }

                return GrpcServerTelemetryContext::new(
                    MyTelemetryContext::Multiple(ids),
                    addr,
                    method.to_string(),
                );
            } else {
                if let Ok(process_id) = process_id.parse::<i64>() {
                    return GrpcServerTelemetryContext::new(
                        MyTelemetryContext::Single(process_id),
                        addr,
                        method.to_string(),
                    );
                }
            }
        }
    }

    return GrpcServerTelemetryContext::new(MyTelemetryContext::new(), addr, method.to_string());
}

fn has_multiple_ids(src: &[u8]) -> bool {
    for b in src {
        if *b == b',' {
            return true;
        }
    }

    false
}

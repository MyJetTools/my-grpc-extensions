#[cfg(feature = "with-telemetry")]
use my_telemetry::MyTelemetryContext;
use my_telemetry::TelemetryEventTag;
use rust_extensions::date_time::DateTimeAsMicroseconds;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

type TelemetryResponseStream<TItem> = Pin<
    Box<dyn futures_util::Stream<Item = Result<TItem, tonic::Status>> + Send + Sync + 'static>,
>;

/// Writes the telemetry event of a request when it is dropped: a success, unless the request was
/// marked as failed with `set_error` or the handler panicked.
pub struct GrpcServerTelemetryContext {
    ctx: Option<MyTelemetryContext>,
    pub started: DateTimeAsMicroseconds,
    addr: Option<SocketAddr>,
    method: Option<String>,
    fail: Mutex<Option<String>>,
}

impl GrpcServerTelemetryContext {
    pub fn new(ctx: MyTelemetryContext, addr: Option<SocketAddr>, method: String) -> Self {
        Self {
            ctx: Some(ctx),
            started: DateTimeAsMicroseconds::now(),
            addr,
            method: Some(method),
            fail: Mutex::new(None),
        }
    }

    pub fn get_ctx(&self) -> &MyTelemetryContext {
        if self.ctx.is_none() {
            panic!("MyTelemetry context is not set");
        }
        self.ctx.as_ref().unwrap()
    }

    /// Marks the request as failed, so the event is written as a fail instead of a success.
    pub fn set_error(&self, status: &tonic::Status) {
        let fail = format!("{:?}: {}", status.code(), status.message());

        match self.fail.lock() {
            Ok(mut access) => *access = Some(fail),
            Err(poisoned) => *poisoned.into_inner() = Some(fail),
        }
    }

    /// Moves the context into the response stream, so the event is written when the stream ends
    /// and covers the whole response, not only the handler which has set it up. A stream which
    /// yields an error is written as a fail.
    pub fn track_stream<TItem: 'static>(
        self,
        result: Result<tonic::Response<TelemetryResponseStream<TItem>>, tonic::Status>,
    ) -> Result<tonic::Response<TelemetryResponseStream<TItem>>, tonic::Status> {
        match result {
            Ok(response) => Ok(response.map(|stream| {
                let stream: TelemetryResponseStream<TItem> = Box::pin(TelemetryTrackedStream {
                    stream,
                    telemetry: self,
                });
                stream
            })),
            Err(status) => {
                self.set_error(&status);
                Err(status)
            }
        }
    }
}

impl Drop for GrpcServerTelemetryContext {
    fn drop(&mut self) {
        if !my_telemetry::TELEMETRY_INTERFACE.is_telemetry_set_up() {
            return;
        }

        let tags = if let Some(addr) = self.addr {
            vec![TelemetryEventTag {
                key: "ip".to_string(),
                value: addr.ip().to_string(),
            }]
            .into()
        } else {
            None
        };

        let started = self.started;

        let method = self.method.take();

        let fail = match self.fail.get_mut() {
            Ok(fail) => fail.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };

        // Dropped by unwinding: the handler panicked, so nothing could mark the request as failed.
        let fail = match fail {
            Some(fail) => Some(fail),
            None if std::thread::panicking() => Some("Panic".to_string()),
            None => None,
        };

        if let Some(ctx) = self.ctx.take() {
            if let Some(method) = method {
                match fail {
                    Some(fail) => my_telemetry::TELEMETRY_INTERFACE.write_fail(
                        &ctx,
                        started,
                        format!("GRPC: {}", method),
                        fail,
                        tags,
                    ),
                    None => my_telemetry::TELEMETRY_INTERFACE.write_success(
                        &ctx,
                        started,
                        format!("GRPC: {}", method),
                        "done".to_string(),
                        tags,
                    ),
                }
            }
        }
    }
}

// The response stream which owns the telemetry context of its request.
struct TelemetryTrackedStream<TItem> {
    stream: TelemetryResponseStream<TItem>,
    telemetry: GrpcServerTelemetryContext,
}

impl<TItem> futures_util::Stream for TelemetryTrackedStream<TItem> {
    type Item = Result<TItem, tonic::Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        let result = this.stream.as_mut().poll_next(cx);

        if let Poll::Ready(Some(Err(status))) = &result {
            this.telemetry.set_error(status);
        }

        result
    }
}

pub fn get_telemetry(
    metadata: &tonic::metadata::MetadataMap,
    addr: Option<SocketAddr>,
    method: &str,
) -> GrpcServerTelemetryContext {
    let ctx = match metadata.get("process-id") {
        Some(process_id) => match std::str::from_utf8(process_id.as_bytes()) {
            Ok(process_id) => parse_process_id(process_id),
            Err(_) => MyTelemetryContext::create_empty(),
        },
        None => MyTelemetryContext::create_empty(),
    };

    GrpcServerTelemetryContext::new(ctx, addr, method.to_string())
}

// The header comes from the caller, so anything can be in it. Segments which are not a number are
// skipped, and what is left decides the kind of the context: `Multiple` with no ids must never be
// built - my-telemetry panics on it when the event is written.
fn parse_process_id(process_id: &str) -> MyTelemetryContext {
    let mut ids: Vec<i64> = process_id
        .split(',')
        .filter_map(|itm| itm.trim().parse().ok())
        .collect();

    match ids.len() {
        0 => MyTelemetryContext::create_empty(),
        1 => MyTelemetryContext::Single(ids.remove(0)),
        _ => MyTelemetryContext::Multiple(ids),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(process_id: &str) -> String {
        format!("{:?}", parse_process_id(process_id))
    }

    #[test]
    fn test_single_id() {
        assert_eq!(parse("42"), "Single(42)");
    }

    #[test]
    fn test_multiple_ids() {
        assert_eq!(parse("1,2,3"), "Multiple([1, 2, 3])");
    }

    #[test]
    fn test_no_ids_between_commas_is_empty() {
        assert_eq!(parse(","), "Empty");
        assert_eq!(parse("abc,def"), "Empty");
        assert_eq!(parse(""), "Empty");
        assert_eq!(parse("abc"), "Empty");
    }

    #[test]
    fn test_one_id_left_after_skipping_is_single() {
        assert_eq!(parse("5,"), "Single(5)");
        assert_eq!(parse("abc,7"), "Single(7)");
    }

    #[test]
    fn test_broken_segments_are_skipped() {
        assert_eq!(parse("1,abc,2"), "Multiple([1, 2])");
        assert_eq!(parse("1, 2"), "Multiple([1, 2])");
    }

    fn new_ctx(process_id: i64, method: &str) -> GrpcServerTelemetryContext {
        GrpcServerTelemetryContext::new(
            MyTelemetryContext::Single(process_id),
            None,
            method.to_string(),
        )
    }

    // (process_id, data, success, fail)
    fn take_events() -> Vec<(i64, String, Option<String>, Option<String>)> {
        my_telemetry::TELEMETRY_INTERFACE
            .telemetry_collector
            .lock()
            .get_events()
            .unwrap_or_default()
            .into_iter()
            .map(|event| (event.process_id, event.data, event.success, event.fail))
            .collect()
    }

    // The collector is shared by the whole process and reading drains it, so this is the only test
    // which reads it.
    #[tokio::test]
    async fn test_what_is_written_on_drop() {
        use futures_util::StreamExt;

        my_telemetry::TELEMETRY_INTERFACE
            .writer_is_set
            .store(true, std::sync::atomic::Ordering::Relaxed);

        drop(new_ctx(1, "Done"));

        let failed = new_ctx(2, "Failed");
        failed.set_error(&tonic::Status::not_found("no user"));
        drop(failed);

        assert_eq!(
            take_events(),
            vec![
                (1, "GRPC: Done".to_string(), Some("done".to_string()), None),
                (
                    2,
                    "GRPC: Failed".to_string(),
                    None,
                    Some("NotFound: no user".to_string())
                ),
            ]
        );

        let stream: TelemetryResponseStream<u32> =
            Box::pin(futures_util::stream::iter(vec![Ok(1), Ok(2)]));
        let mut stream = new_ctx(3, "Stream")
            .track_stream(Ok(tonic::Response::new(stream)))
            .unwrap()
            .into_inner();

        // The handler has returned, the response has not ended yet.
        assert_eq!(take_events(), vec![]);

        while stream.next().await.is_some() {}
        drop(stream);

        assert_eq!(
            take_events(),
            vec![(3, "GRPC: Stream".to_string(), Some("done".to_string()), None)]
        );

        let stream: TelemetryResponseStream<u32> = Box::pin(futures_util::stream::iter(vec![
            Ok(1),
            Err(tonic::Status::aborted("gave up")),
        ]));
        let mut stream = new_ctx(4, "FailedStream")
            .track_stream(Ok(tonic::Response::new(stream)))
            .unwrap()
            .into_inner();
        while stream.next().await.is_some() {}
        drop(stream);

        let not_started = new_ctx(5, "NotStartedStream")
            .track_stream::<u32>(Err(tonic::Status::internal("Result is already taken")));
        assert!(not_started.is_err());

        assert_eq!(
            take_events(),
            vec![
                (
                    4,
                    "GRPC: FailedStream".to_string(),
                    None,
                    Some("Aborted: gave up".to_string())
                ),
                (
                    5,
                    "GRPC: NotStartedStream".to_string(),
                    None,
                    Some("Internal: Result is already taken".to_string())
                ),
            ]
        );
    }

    #[test]
    fn test_header_without_ids_gives_empty_context() {
        let mut metadata = tonic::metadata::MetadataMap::new();
        metadata.insert("process-id", ",".parse().unwrap());

        let telemetry = get_telemetry(&metadata, None, "Test");

        assert!(matches!(telemetry.get_ctx(), MyTelemetryContext::Empty));
    }
}

use std::time::Duration;

use tokio_stream::StreamExt;

pub struct StreamedRequestReader<TItem> {
    stream: tonic::Streaming<TItem>,
    timeout: Duration,
}

impl<TItem> StreamedRequestReader<TItem> {
    pub fn new(stream: tonic::Streaming<TItem>) -> Self {
        Self {
            stream,
            timeout: Duration::from_secs(20),
        }
    }

    pub fn set_timeout(mut self, value: Duration) -> Self {
        self.timeout = value;
        self
    }

    pub async fn get_next(&mut self) -> Option<tonic::Result<TItem>> {
        let future = self.stream.next();

        let result = tokio::time::timeout(self.timeout, future).await;

        if result.is_err() {
            return Some(Err(tonic::Status::unavailable(format!(
                "Timeout: {:?}",
                self.timeout
            ))));
        }

        result.unwrap()
    }

    pub async fn into_vec(mut self) -> tonic::Result<Vec<TItem>> {
        let mut result = Vec::new();

        while let Some(item) = self.get_next().await {
            let item = item?;
            result.push(item);
        }

        Ok(result)
    }
}

impl<TItem> Into<StreamedRequestReader<TItem>> for tonic::Streaming<TItem> {
    fn into(self) -> StreamedRequestReader<TItem> {
        StreamedRequestReader::new(self)
    }
}

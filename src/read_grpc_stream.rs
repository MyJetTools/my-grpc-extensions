use std::time::Duration;

use futures_util::StreamExt;
use rust_extensions::lazy::LazyVec;

#[derive(Debug)]
pub enum ReadStreamError {
    Timeout,
    TonicError(tonic::Status),
}

pub async fn as_vec<T>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
) -> Result<Option<Vec<T>>, ReadStreamError> {
    let mut result = LazyVec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await;

        if response.is_err() {
            return Err(ReadStreamError::Timeout);
        }

        match response.unwrap() {
            Some(item) => match item {
                Ok(item) => {
                    result.add(item);
                }
                Err(err) => Err(ReadStreamError::TonicError(err))?,
            },
            None => {
                return Ok(result.get_result());
            }
        }
    }
}

pub async fn first_or_none<T>(
    mut streaming: tonic::Streaming<T>,
    timeout: Duration,
) -> Result<Option<T>, ReadStreamError> {
    let response = tokio::time::timeout(timeout, streaming.next()).await;

    if response.is_err() {
        return Err(ReadStreamError::Timeout);
    }

    match response.unwrap() {
        Some(item) => match item {
            Ok(item) => {
                return Ok(Some(item));
            }
            Err(err) => Err(ReadStreamError::TonicError(err))?,
        },
        None => {
            return Ok(None);
        }
    }
}

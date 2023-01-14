use std::{collections::HashMap, time::Duration};

use futures_util::StreamExt;
use rust_extensions::lazy::{LazyHashMap, LazyVec};

use crate::GrpcReadError;

pub async fn as_vec<T>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
) -> Result<Option<Vec<T>>, GrpcReadError> {
    let mut result = LazyVec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    result.add(item);
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
            },
            None => {
                return Ok(result.get_result());
            }
        }
    }
}

pub async fn as_hash_map<TSrc, TKey, TValue, TGetKey: Fn(TSrc) -> (TKey, TValue)>(
    mut stream_to_read: tonic::Streaming<TSrc>,
    get_key: &TGetKey,
    timeout: Duration,
) -> Result<Option<HashMap<TKey, TValue>>, GrpcReadError>
where
    TKey: std::cmp::Eq + core::hash::Hash + Clone,
{
    let mut result = LazyHashMap::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    let (key, value) = get_key(item);
                    result.insert(key, value);
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
            },
            None => {
                return Ok(result.get_result());
            }
        }
    }
}

pub async fn as_vec_with_transformation<T, TDest, TFn: Fn(T) -> TDest>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
    transform: &TFn,
) -> Result<Option<Vec<TDest>>, GrpcReadError> {
    let mut result = LazyVec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    let item = transform(item);
                    result.add(item);
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
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
) -> Result<Option<T>, GrpcReadError> {
    let response = tokio::time::timeout(timeout, streaming.next()).await?;

    match response {
        Some(item) => match item {
            Ok(item) => {
                return Ok(Some(item));
            }
            Err(err) => Err(GrpcReadError::TonicStatus(err))?,
        },
        None => {
            return Ok(None);
        }
    }
}

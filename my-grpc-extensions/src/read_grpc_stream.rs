use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use futures_util::StreamExt;

use crate::GrpcReadError;

pub async fn as_vec<T, TDest: From<T>>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
) -> Result<Vec<TDest>, GrpcReadError> {
    let mut result = Vec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    result.push(item.into());
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
            },
            None => {
                return Ok(result);
            }
        }
    }
}

pub async fn as_hash_map<TSrc, TKey, TValue, TGetKey: Fn(TSrc) -> (TKey, TValue)>(
    mut stream_to_read: tonic::Streaming<TSrc>,
    get_key: &TGetKey,
    timeout: Duration,
) -> Result<HashMap<TKey, TValue>, GrpcReadError>
where
    TKey: std::cmp::Eq + core::hash::Hash + Clone,
{
    let mut result = HashMap::new();

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
                return Ok(result);
            }
        }
    }
}

pub async fn as_b_tree_map<TSrc, TKey: Ord, TValue, TGetKey: Fn(TSrc) -> (TKey, TValue)>(
    mut stream_to_read: tonic::Streaming<TSrc>,
    get_key: &TGetKey,
    timeout: Duration,
) -> Result<BTreeMap<TKey, TValue>, GrpcReadError>
where
    TKey: core::hash::Hash + Clone,
{
    let mut result = BTreeMap::new();

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
                return Ok(result);
            }
        }
    }
}

pub async fn as_vec_with_transformation<T, TDest, TFn: Fn(T) -> TDest>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
    transform: &TFn,
) -> Result<Vec<TDest>, GrpcReadError> {
    let mut result = Vec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    let item = transform(item);
                    result.push(item);
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
            },
            None => {
                return Ok(result);
            }
        }
    }
}

pub async fn as_vec_with_transformation_and_filter<T, TDest, TFn: Fn(T) -> Option<TDest>>(
    mut stream_to_read: tonic::Streaming<T>,
    timeout: Duration,
    transform: &TFn,
) -> Result<Vec<TDest>, GrpcReadError> {
    let mut result = Vec::new();

    loop {
        let response = tokio::time::timeout(timeout, stream_to_read.next()).await?;

        match response {
            Some(item) => match item {
                Ok(item) => {
                    let item = transform(item);
                    if let Some(item) = item {
                        result.push(item);
                    }
                }
                Err(err) => Err(GrpcReadError::TonicStatus(err))?,
            },
            None => {
                return Ok(result);
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

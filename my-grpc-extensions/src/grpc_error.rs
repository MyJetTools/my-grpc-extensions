use crate::GrpcReadError;

/// Error a gRPC server handler is allowed to return when `generate_server!` is invoked with
/// `with_error: true`.
///
/// It is a thin wrapper around `tonic::Status` - the status is kept as is, so a status which
/// arrived from an upstream service travels through the handler and reaches the caller with its
/// original code and message. The generated handler converts it back into `tonic::Status` with `?`.
///
/// ```ignore
/// async fn get_user(
///     app: &Arc<AppContext>,
///     request: GetUserRequest,
/// ) -> Result<GetUserResponse, GrpcError> {
///     // GrpcReadError of the client call is converted automatically
///     let user = app.mt4_bridge.get_user(request.into(), ctx).await?;
///     Ok(user.into())
/// }
/// ```
#[derive(Debug)]
pub struct GrpcError(tonic::Status);

impl GrpcError {
    pub fn new(status: tonic::Status) -> Self {
        Self(status)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self(tonic::Status::unavailable(message.into()))
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self(tonic::Status::not_found(message.into()))
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self(tonic::Status::internal(message.into()))
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self(tonic::Status::invalid_argument(message.into()))
    }

    pub fn code(&self) -> tonic::Code {
        self.0.code()
    }

    pub fn message(&self) -> &str {
        self.0.message()
    }

    pub fn as_status(&self) -> &tonic::Status {
        &self.0
    }

    pub fn into_status(self) -> tonic::Status {
        self.0
    }
}

impl From<GrpcError> for tonic::Status {
    fn from(src: GrpcError) -> Self {
        src.0
    }
}

impl From<tonic::Status> for GrpcError {
    fn from(src: tonic::Status) -> Self {
        Self(src)
    }
}

impl From<GrpcReadError> for GrpcError {
    fn from(src: GrpcReadError) -> Self {
        match src {
            // The status of the upstream service is passed through untouched - this is what carries
            // a meaningful error from a bridge up to the caller.
            GrpcReadError::TonicStatus(status) => Self(status),
            GrpcReadError::Timeout => Self(tonic::Status::deadline_exceeded("Grpc request timeout")),
            GrpcReadError::TransportError(err) => {
                Self(tonic::Status::unavailable(format!("Transport error: {}", err)))
            }
            GrpcReadError::Other(msg) => Self(tonic::Status::internal(msg)),
        }
    }
}

impl From<String> for GrpcError {
    fn from(src: String) -> Self {
        Self::internal(src)
    }
}

impl From<&'_ str> for GrpcError {
    fn from(src: &'_ str) -> Self {
        Self::internal(src.to_string())
    }
}

impl std::fmt::Display for GrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for GrpcError {}

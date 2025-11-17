use thiserror::Error;

#[derive(Error, Debug)]
pub enum ModuleError {
    #[error("compositor IPC error: {0}")]
    CompositorIpc(#[source] std::io::Error),

    #[error("compositor returned error: {0}")]
    CompositorReply(String),

    #[error("unexpected compositor response; expected {expected}: {actual:?}")]
    UnexpectedResponse {
        expected: &'static str,
        actual: Box<niri_ipc::Response>,
    },

    #[error("window snapshot channel closed")]
    SnapshotChannelClosed,
}

impl ModuleError {
    pub fn unexpected_response(expected: &'static str, actual: niri_ipc::Response) -> Self {
        Self::UnexpectedResponse {
            expected,
            actual: Box::new(actual),
        }
    }
}

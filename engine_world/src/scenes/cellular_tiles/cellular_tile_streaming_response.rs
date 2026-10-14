// Copyright Rob Gage 2026

/// Completed staging buffer mapping returned to the cellular tile streaming state
pub struct CellularTileStreamingResponse {
    /// Identifier of the matching streaming request
    pub streaming_request_identifier: u64,
    /// Staging buffer mapping result
    pub result: Result<(), wgpu::BufferAsyncError>,
}

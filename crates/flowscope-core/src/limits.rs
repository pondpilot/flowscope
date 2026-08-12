//! Resource limits for FlowScope analysis requests.

/// Maximum UTF-8 size of one SQL source: 10 MiB.
///
/// An inline `AnalyzeRequest::sql` value and each `FileSource::content` value
/// are separate sources.
pub(crate) const MAX_ANALYSIS_SOURCE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum aggregate UTF-8 size of all SQL sources in one analysis: 100 MiB.
///
/// The total includes inline SQL plus every file's content.
pub(crate) const MAX_ANALYSIS_TOTAL_BYTES: usize = 100 * 1024 * 1024;

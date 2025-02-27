//! # I/O Retry System
//!
//! This module provides a robust I/O error handling system with smart retry capabilities
//! specifically designed for filesystem operations on Linux. It distinguishes between
//! fatal errors (that cannot be recovered from) and transient errors (that may resolve
//! with retries).
//!
//! ## Key Features
//!
//! - Intelligent categorization of I/O errors into fatal and transient types
//! - Exponential backoff retry mechanism with configurable parameters
//! - Rich error types that implement the standard `Error` trait
//! - Extension traits for easy integration with existing code
//! - Support for providing context with errors
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use docufort::io_retry::{retry_io_operation, RetryConfig, RetryIoResultExt};
//! use std::fs::File;
//! use std::io::Write;
//!
//! // Method 1: Use the retry extension trait
//! let result = (|| {
//!     let mut file = File::create("important_data.log")?;
//!     file.write_all(b"Critical information")?;
//!     file.flush()?;
//!     Ok(())
//! }).retry();
//!
//! // Method 2: Use the function directly with custom config
//! let config = RetryConfig {
//!     max_attempts: 10,
//!     initial_backoff_ms: 50,
//!     // ... other settings
//!     ..Default::default()
//! };
//!
//! let result = retry_io_operation(
//!     || {
//!         let mut file = File::create("important_data.log")?;
//!         file.write_all(b"Critical information")?;
//!         file.flush()?;
//!         Ok(())
//!     },
//!     &config,
//! );
//!
//! // Handle the result appropriately
//! match result {
//!     Ok(_) => println!("Operation succeeded"),
//!     Err(err) if err.is_fatal() => println!("Fatal error: {}", err),
//!     Err(err) => println!("Transient error after max retries: {}", err),
//! }
//! ```
//!
//! // Method 3: Use the RetryingFile wrapper
//! ```rust,no_run
//! use docufort::io_retry::RetryingFile;
//! use docufort::io_retry::RetryConfig;
//! use std::fs::File;
//! use std::io::Write;
//!
//! let file = File::create("important_data.log")?;
//! let mut retrying_file = RetryingFile::new(file);
//! retrying_file.write_all(b"Critical information")?;
//! retrying_file.flush()?;
//!
//! // Or with configuration
//! let file = File::create("important_data.log")?;
//! let mut retrying_file = RetryingFile::with_config(file, RetryConfig {
//!     max_attempts: 10,
//!     initial_backoff_ms: 10,
//!     ..Default::default()
//! });
//! retrying_file.write_all(b"Critical information")?;
//! retrying_file.flush()?;
//! Ok::<(), std::io::Error>(())
//! ```
//!
//! ## Error Handling Patterns
//!
//! ### Context-Rich Errors
//!
//! ```rust
//! use docufort::io_retry::{self,IoResultExt};
//! use std::fs::File;
//! use std::io::Write;
//!
//! fn write_to_log(data: &[u8]) -> Result<(), io_retry::FileSystemError> {
//!     let mut file = File::create("app.log")
//!         .or_categorize(|| "Failed to create log file".to_string())?;
//!
//!     file.write_all(data)
//!         .or_categorize(|| format!("Failed to write {} bytes to log", data.len()))?;
//!
//!     file.flush()
//!         .or_categorize(|| "Failed to flush log file".to_string())?;
//!
//!     Ok(())
//! }
//! ```
//!
//!
//! ## Configuring Retries
//!
//! The retry behavior can be fine-tuned using `RetryConfig`:
//!
//! ```rust,no_run
//! use docufort::io_retry::{RetryConfig, RetryIoResultExt};
//! use std::time::Duration;
//! use std::fs::File;
//! use std::io::Write;
//!
//! let some_io_operation = (|| {
//!     let mut file = File::create("important_data.log")?;
//!     file.write_all(b"Critical information")?;
//!     file.flush()?;
//!     Ok(())
//! });
//!
//! // Configure for critical operations (more retries, longer timeout)
//! let critical_config = RetryConfig {
//!     max_attempts: 15,
//!     initial_backoff_ms: 20,
//!     max_backoff_ms: 5000,
//!     backoff_multiplier: 2.0,
//!     max_tot_dur_secs: 30,
//! };
//!
//! // Configure for less important operations (fewer retries, shorter timeout)
//! let normal_config = RetryConfig {
//!     max_attempts: 5,
//!     initial_backoff_ms: 50,
//!     max_backoff_ms: 2000,
//!     backoff_multiplier: 1.5,
//!     max_tot_dur_secs: 10,
//! };
//!
//! // Use with the retry extension
//! let result = some_io_operation.retry_with_config(&critical_config);
//! ```

use std::io::{self, Error as IoError, ErrorKind, Read, Seek, SeekFrom, Write};
use std::time::{Duration, Instant};
use std::thread;
use std::fmt;
use std::error::Error;

use crate::FileLike;



/// A wrapper around any Read + Write + Seek type that adds transparent retry functionality.
///
/// This wrapper implements standard I/O traits (Read, Write, Seek) and transparently
/// applies retry logic when operations fail with transient errors. It only returns
/// errors to the caller when a fatal error occurs or when retries are exhausted.
///
/// Usage:
/// ```rust,no_run
/// use docufort::io_retry::RetryingFile;
/// use std::fs::File;
/// use std::io::Write;
/// use std::io::Result;
/// let file = std::fs::OpenOptions::new()
///     .read(true)
///     .write(true)
///     .create(true)
///     .open("my_log_file.bin")?;
///
/// let mut retrying_file = RetryingFile::new(file);
/// // Now use retrying_file as you would a normal file
/// retrying_file.write_all(b"some data")?;
/// Ok::<(), std::io::Error>(())
/// ```
pub struct RetryingFile<T> {
    inner: T,
    retry_config: RetryConfig,
}

impl<T> RetryingFile<T> {
    /// Create a new RetryingFile with default retry configuration
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new RetryingFile with custom retry configuration
    pub fn with_config(inner: T, retry_config: RetryConfig) -> Self {
        Self {
            inner,
            retry_config,
        }
    }

    /// Get a reference to the inner file
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner file
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Unwrap this RetryingFile, returning the inner file
    pub fn into_inner(self) -> T {
        self.inner
    }
}

// Implement Read for RetryingFile
impl<T: Read> Read for RetryingFile<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inner = &mut self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.read(buf), config).map_err(|e|e.into())
    }
}

// Implement Write for RetryingFile
impl<T: Write> Write for RetryingFile<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let inner = &mut self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.write(buf), config).map_err(|e|e.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        let inner = &mut self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.flush(), config).map_err(|e|e.into())
    }
}

// Implement Seek for RetryingFile
impl<T: Seek> Seek for RetryingFile<T> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let inner = &mut self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.seek(pos), config).map_err(|e|e.into())
    }
}

impl<T: FileLike> FileLike for RetryingFile<T> {
    fn truncate(&mut self, len: u64)->std::io::Result<()> {
        let inner = &mut self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.truncate(len), config).map_err(|e|e.into())
    }

    fn len(&self)->std::io::Result<u64> {
        let inner = &self.inner;
        let config = &self.retry_config;
        retry_io_operation(|| inner.len(), config).map_err(|e|e.into())
    }
}


impl From<FileSystemError> for io::Error {
    fn from(err: FileSystemError) -> Self {
        match err {
            FileSystemError::Fatal(fatal_err) => {
                match fatal_err {
                    FatalError::PermissionDenied =>
                        io::Error::new(ErrorKind::PermissionDenied, fatal_err),
                    FatalError::ReadOnlyFileSystem =>
                        io::Error::new(ErrorKind::PermissionDenied, fatal_err),
                    FatalError::NoSpace =>
                        io::Error::new(ErrorKind::Other, fatal_err),
                    FatalError::FileTooLarge =>
                        io::Error::new(ErrorKind::Other, fatal_err),
                    FatalError::HardwareFailure =>
                        io::Error::new(ErrorKind::Other, fatal_err),
                    FatalError::InvalidFileDescriptor =>
                        io::Error::new(ErrorKind::Other, fatal_err),
                    FatalError::FileNotFound =>
                        io::Error::new(ErrorKind::NotFound, fatal_err),
                    FatalError::QuotaExceeded =>
                        io::Error::new(ErrorKind::Other, fatal_err),
                    FatalError::IoError(io_err) => io_err,
                    FatalError::Other(msg) =>
                        io::Error::new(ErrorKind::Other, msg),
                }
            },
            FileSystemError::TransientFailure(transient_err) => {
                // If we're reporting a transient error, it means retries were exhausted
                // We should indicate this is a possibly retriable error but our retries failed
                match transient_err {
                    TransientError::TemporarilyUnavailable =>
                        io::Error::new(ErrorKind::WouldBlock, transient_err),
                    TransientError::Interrupted =>
                        io::Error::new(ErrorKind::Interrupted, transient_err),
                    TransientError::NetworkFileSystemIssue =>
                        io::Error::new(ErrorKind::Other, transient_err),
                    TransientError::TooManyOpenFiles =>
                        io::Error::new(ErrorKind::Other, transient_err),
                    TransientError::LockContention =>
                        io::Error::new(ErrorKind::WouldBlock, transient_err),
                    TransientError::IoError(io_err) => io_err,
                    TransientError::Other(msg) =>
                        io::Error::new(ErrorKind::Other, msg),
                }
            }
        }
    }
}



/// Represents the outcome of a filesystem operation with detailed error classification
#[derive(Debug)]
pub enum FileSystemError {
    /// Fatal errors that indicate the operation cannot succeed with retries
    Fatal(FatalError),

    /// Transient errors that might succeed with retries but ultimately failed
    TransientFailure(TransientError),
}

impl FileSystemError {
    /// Returns true if this is a fatal error
    pub fn is_fatal(&self) -> bool {
        matches!(self, FileSystemError::Fatal(_))
    }

    /// Returns true if this is a transient error
    pub fn is_transient(&self) -> bool {
        matches!(self, FileSystemError::TransientFailure(_))
    }

    /// Unwraps the fatal error if this is a fatal error
    pub fn unwrap_fatal(self) -> Result<FatalError, Self> {
        match self {
            FileSystemError::Fatal(err) => Ok(err),
            _ => Err(self),
        }
    }

    /// Unwraps the transient error if this is a transient error
    pub fn unwrap_transient(self) -> Result<TransientError, Self> {
        match self {
            FileSystemError::TransientFailure(err) => Ok(err),
            _ => Err(self),
        }
    }
}

impl Error for FileSystemError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FileSystemError::Fatal(err) => Some(err),
            FileSystemError::TransientFailure(err) => Some(err),
        }
    }
}

impl fmt::Display for FileSystemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileSystemError::Fatal(err) => write!(f, "Fatal I/O error: {}", err),
            FileSystemError::TransientFailure(err) => write!(f, "Transient I/O error: {}", err),
        }
    }
}

impl From<std::io::Error> for FileSystemError {
    fn from(error: std::io::Error) -> Self {
        categorize_io_error(error)
    }
}

/// Represents errors that are fatal and cannot be recovered from without external intervention
#[derive(Debug)]
pub enum FatalError {
    /// Permission denied or insufficient privileges
    PermissionDenied,
    /// The file system is read-only
    ReadOnlyFileSystem,
    /// Not enough space on the device
    NoSpace,
    /// The file is too large for the filesystem
    FileTooLarge,
    /// Hardware I/O error indicating device failure
    HardwareFailure,
    /// Invalid file descriptor
    InvalidFileDescriptor,
    /// The file does not exist
    FileNotFound,
    /// File system quota exceeded
    QuotaExceeded,
    /// An unspecified fatal error
    Other(String),
    /// Wrapped IO error that was determined to be fatal
    IoError(IoError),
}

impl Error for FatalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FatalError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for FatalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FatalError::PermissionDenied => write!(f, "Permission denied"),
            FatalError::ReadOnlyFileSystem => write!(f, "Filesystem is read-only"),
            FatalError::NoSpace => write!(f, "No space left on device"),
            FatalError::FileTooLarge => write!(f, "File too large"),
            FatalError::HardwareFailure => write!(f, "Hardware I/O error"),
            FatalError::InvalidFileDescriptor => write!(f, "Invalid file descriptor"),
            FatalError::FileNotFound => write!(f, "File not found"),
            FatalError::QuotaExceeded => write!(f, "Disk quota exceeded"),
            FatalError::Other(msg) => write!(f, "{}", msg),
            FatalError::IoError(err) => write!(f, "I/O error: {}", err),
        }
    }
}

/// Represents errors that might be transient and could succeed with retries
#[derive(Debug)]
pub enum TransientError {
    /// Resource temporarily unavailable (would block)
    TemporarilyUnavailable,
    /// Operation interrupted by a signal
    Interrupted,
    /// Temporary network filesystem issue
    NetworkFileSystemIssue,
    /// Too many open files (system or process limit)
    TooManyOpenFiles,
    /// Lock contention
    LockContention,
    /// An unspecified transient error that failed after max retries
    Other(String),
    /// Wrapped IO error that was determined to be transient
    IoError(IoError),
}

impl Error for TransientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TransientError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for TransientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransientError::TemporarilyUnavailable => write!(f, "Resource temporarily unavailable"),
            TransientError::Interrupted => write!(f, "Operation interrupted"),
            TransientError::NetworkFileSystemIssue => write!(f, "Network filesystem issue"),
            TransientError::TooManyOpenFiles => write!(f, "Too many open files"),
            TransientError::LockContention => write!(f, "Lock contention"),
            TransientError::Other(msg) => write!(f, "{}", msg),
            TransientError::IoError(err) => write!(f, "I/O error: {}", err),
        }
    }
}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,
    /// Backoff multiplier for exponential increase
    pub backoff_multiplier: f64,
    /// Maximum total duration for all retries combined
    pub max_tot_dur_secs: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            max_attempts: 5,
            initial_backoff_ms: 50,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
            max_tot_dur_secs: 30,
        }
    }
}

/// Categorizes an IO error as either fatal or transient
pub fn categorize_io_error(error: IoError) -> FileSystemError {
    match error.kind() {
        // Fatal errors
        ErrorKind::PermissionDenied => FileSystemError::Fatal(FatalError::PermissionDenied),
        ErrorKind::NotFound => FileSystemError::Fatal(FatalError::FileNotFound),

        // Potentially recoverable errors
        ErrorKind::Interrupted => FileSystemError::TransientFailure(TransientError::Interrupted),
        ErrorKind::WouldBlock => FileSystemError::TransientFailure(TransientError::TemporarilyUnavailable),

        // For other error kinds, we need to examine the OS error code
        _ => {
            #[cfg(unix)]
            {
                if let Some(os_error) = error.raw_os_error() {
                    #[allow(unreachable_patterns)] //For EAGAIN and EWOULDBLOCK
                    match os_error {
                        // Fatal errors
                        libc::EROFS => FileSystemError::Fatal(FatalError::ReadOnlyFileSystem),
                        libc::ENOSPC => FileSystemError::Fatal(FatalError::NoSpace),
                        libc::EFBIG => FileSystemError::Fatal(FatalError::FileTooLarge),
                        libc::EBADF => FileSystemError::Fatal(FatalError::InvalidFileDescriptor),
                        libc::EDQUOT => FileSystemError::Fatal(FatalError::QuotaExceeded),

                        // Potentially hardware related but could be examined more
                        libc::EIO => {
                            // General I/O error - This could be transient in some cases (like NFS)
                            // but for local filesystems it's often fatal
                            FileSystemError::Fatal(FatalError::HardwareFailure)
                        }

                        // Potentially recoverable errors
                        // On Linux, EAGAIN and EWOULDBLOCK are identical, but we include both for clarity
                        libc::EAGAIN | libc::EWOULDBLOCK => {
                            FileSystemError::TransientFailure(TransientError::TemporarilyUnavailable)
                        }
                        libc::EINTR => FileSystemError::TransientFailure(TransientError::Interrupted),
                        libc::ENFILE | libc::EMFILE => {
                            FileSystemError::TransientFailure(TransientError::TooManyOpenFiles)
                        }
                        libc::EDEADLK => FileSystemError::TransientFailure(TransientError::LockContention),

                        // Default case
                        _ => FileSystemError::TransientFailure(TransientError::Other(format!(
                            "Unknown OS error: {}", os_error
                        ))),
                    }
                } else {
                    FileSystemError::TransientFailure(TransientError::IoError(error))
                }
            }

            #[cfg(not(unix))]
            {
                // For non-Unix platforms, we have less specific information
                FileSystemError::TransientFailure(TransientError::IoError(error))
            }
        }
    }
}

/// Attempts an IO operation with exponential backoff and jitter
///
/// This function will retry the provided operation according to the retry config:
/// - Distinguishes between fatal and transient errors
/// - Uses exponential backoff with jitter for transient errors
/// - Returns immediately for fatal errors
/// - Respects maximum retry attempts and total timeout
///
/// # Arguments
/// * `operation` - The IO operation to attempt
/// * `config` - Configuration for the retry behavior
///
/// # Returns
/// * `Ok(T)` - The operation succeeded
/// * `Err(FileSystemError)` - The operation failed, with details about the failure
pub fn retry_io_operation<T, F>(
    mut operation: F,
    config: &RetryConfig,
) -> Result<T, FileSystemError>
where
    F: FnMut() -> io::Result<T>,
{
    let start_time = Instant::now();
    let mut current_attempt = 0;
    let mut current_backoff_ms = config.initial_backoff_ms;

    loop {
        match operation() {
            Ok(result) => return Ok(result),
            Err(err) => {
                // First, categorize the error
                let categorized_error = categorize_io_error(err);

                // If it's a fatal error, return immediately
                if let FileSystemError::Fatal(_) = categorized_error {
                    return Err(categorized_error);
                }

                // Otherwise, it's a transient error
                current_attempt += 1;

                // Check if we've exceeded max attempts or total duration
                if current_attempt >= config.max_attempts ||
                    start_time.elapsed().as_secs() >= config.max_tot_dur_secs as u64 {
                    return Err(categorized_error)
                }

                // // Calculate backoff with jitter
                // let mut rng = thread_rng();
                // let jitter = rng.gen_range(
                //     (-config.jitter_factor)..config.jitter_factor
                // );
                // let jittered_backoff = (current_backoff_ms as f64 * (1.0 + jitter)) as u64;

                // Sleep for the backoff period
                thread::sleep(Duration::from_millis(current_backoff_ms));

                // Increase backoff for next attempt, but don't exceed max
                current_backoff_ms = (current_backoff_ms as f64 * config.backoff_multiplier) as u64;
                if current_backoff_ms > config.max_backoff_ms {
                    current_backoff_ms = config.max_backoff_ms;
                }
            }
        }
    }
}

/// Helper trait to extend io::Result with retry capabilities
pub trait RetryIoResultExt<T> {
    /// Retries the operation with default retry configuration
    fn retry(self) -> Result<T, FileSystemError>;

    /// Retries the operation with custom retry configuration
    fn retry_with_config(self, config: &RetryConfig) -> Result<T, FileSystemError>;
}

impl<T, F> RetryIoResultExt<T> for F
where
    F: FnMut() -> io::Result<T>,
{
    fn retry(self) -> Result<T, FileSystemError> {
        retry_io_operation(self, &RetryConfig::default())
    }

    fn retry_with_config(self, config: &RetryConfig) -> Result<T, FileSystemError> {
        retry_io_operation(self, config)
    }
}

// Extension trait for io::Result to convert to our error types
pub trait IoResultExt<T> {
    /// Converts an io::Result to our Result<T, FileSystemError>
    fn into_fs_result(self) -> Result<T, FileSystemError>;

    /// Attempts to execute an operation and classify the error if it fails
    fn or_categorize<F>(self, context: F) -> Result<T, FileSystemError>
    where
        F: FnOnce() -> String;
}

impl<T> IoResultExt<T> for io::Result<T> {
    fn into_fs_result(self) -> Result<T, FileSystemError> {
        self.map_err(|e| e.into())
    }

    fn or_categorize<F>(self, context: F) -> Result<T, FileSystemError>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| {
            let mut fs_err = categorize_io_error(e);

            // Add context to the error message
            match &mut fs_err {
                FileSystemError::Fatal(FatalError::Other(msg)) => {
                    *msg = format!("{}: {}", context(), msg);
                }
                FileSystemError::Fatal(FatalError::IoError(io_err)) => {
                    let err_msg = format!("{}: {}", context(), io_err);
                    fs_err = FileSystemError::Fatal(FatalError::Other(err_msg));
                }
                FileSystemError::TransientFailure(TransientError::Other(msg)) => {
                    *msg = format!("{}: {}", context(), msg);
                }
                FileSystemError::TransientFailure(TransientError::IoError(io_err)) => {
                    let err_msg = format!("{}: {}", context(), io_err);
                    fs_err = FileSystemError::TransientFailure(TransientError::Other(err_msg));
                }
                _ => {}
            }

            fs_err
        })
    }
}










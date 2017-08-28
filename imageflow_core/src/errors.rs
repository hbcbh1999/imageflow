use ::std;
use ::std::fmt;
use ::context::Context;
use std::borrow::Cow;
use std::any::Any;
use std::io::Write;
use std::io;
use std::cmp;
use num::FromPrimitive;

#[macro_export]
macro_rules! here {
    () => (
        ::CodeLocation{ line: line!(), column: column!(), file: file!(), module: module_path!()}
    );
}
#[macro_export]
macro_rules! loc {
    () => (
        concat!(file!(), ":", line!(), ":", column!(), " in ", module_path!())
    );
    ($msg:expr) => (
        concat!($msg, " at\n", file!(), ":", line!(), ":", column!(), " in ", module_path!())
    );
}
#[macro_export]
macro_rules! nerror {
    ($kind:expr) => (
        NodeError{
            kind: $kind,
            message: format!("NodeError {:?}", $kind),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
    ($kind:expr, $fmt:expr) => (
        NodeError{
            kind: $kind,
            message:  format!(concat!("NodeError {:?}: ",$fmt ), $kind,),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
    ($kind:expr, $fmt:expr, $($arg:tt)*) => (
        NodeError{
            kind: $kind,
            message:  format!(concat!("NodeError {:?}: ", $fmt), $kind, $($arg)*),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
}
#[macro_export]
macro_rules! unimpl {
    () => (
        NodeError{
            kind: ::ErrorKind::MethodNotImplemented,
            message: String::new(),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
}

pub type Result<T> = std::result::Result<T, FlowError>;
pub type NResult<T> = ::std::result::Result<T, NodeError>;



#[repr(C)]
#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum ErrorCategory{
    /// No error
    Ok = 0,
    /// Not valid JSON
    JsonMalformed,
    /// Image should have been, but could not be decoded
    ImageMalformed,
    /// No support for decoding this type of image (or subtype)
    ImageTypeNotSupported,
    /// A file or remote resource was not found
    SecondaryResourceNotFound,
    /// The primary file/remote resource for this job was not found
    PrimaryResourceNotFound,
    /// Invalid parameters were found in a operation node
    NodeArgumentInvalid,
    /// The graph is invalid; it may have cycles, or have nodes connected in ways they do not support.
    GraphInvalid,
    /// An operation described in the job is not supported
    ActionNotSupported,
    /// The job could not be completed; the graph could not be executed within a reasonable number of passes.
    NoSolutionFound,
    /// An internal error has occurred; please report this as it could be a bug
    InternalError,
    /// The process was unable to allocate enough memory to finish the job
    OutOfMemory,
    /// An I/O error of some kind occurred (this may be related to file locks or permissions or something else)
    IoError,
    /// An upstream server failed to respond correctly (not a 404, but some other error)
    UpstreamError,
    /// A request to an upstream server timed out
    UpstreamTimeout,
    /// An invalid argument was provided to Imageflow
    ArgumentInvalid,
    /// An operation is forbidden by the active Imageflow security policy
    ActionForbidden,
    /// The imageflow server requires authorization to complete the request
    AuthorizationRequired,
    /// A valid license is needed for the specified job
    LicenseError,
    /// The category of the error is unknown
    Unknown

}
impl ErrorCategory{
    pub fn exit_code(&self) -> i32{
        match *self {
            ErrorCategory::ArgumentInvalid |
            ErrorCategory::GraphInvalid |
            ErrorCategory::ActionNotSupported |
            ErrorCategory::NodeArgumentInvalid => 64, //EX_USAGE
            ErrorCategory::JsonMalformed |
            ErrorCategory::ImageMalformed |
            ErrorCategory::ImageTypeNotSupported  => 65, //EX_DATAERR
            ErrorCategory::SecondaryResourceNotFound |
            ErrorCategory::PrimaryResourceNotFound => 66, // EX_NOINPUT
            ErrorCategory::UpstreamError |
            ErrorCategory::UpstreamTimeout => 69, //EX_UNAVAILABLE
            ErrorCategory::InternalError  |
            ErrorCategory::NoSolutionFound  |
            ErrorCategory::Unknown => 70, //EX_SOFTWARE
            ErrorCategory::OutOfMemory => 71,// EX_TEMPFAIL 75 or EX_OSERR   71 ?
            ErrorCategory::IoError => 74, //EX_IOERR
            ErrorCategory::ActionForbidden => 77, //EX_NOPERM
            ErrorCategory::LicenseError => 402,
            ErrorCategory::AuthorizationRequired => 401,
            ErrorCategory::Ok => 0
        }
    }
    pub fn status_code(&self) -> i32{
        match *self {
            ErrorCategory::Ok => 200,

            ErrorCategory::ArgumentInvalid |
            ErrorCategory::GraphInvalid |
            ErrorCategory::NodeArgumentInvalid |
            ErrorCategory::ActionNotSupported |
            ErrorCategory::JsonMalformed |
            ErrorCategory::ImageMalformed |
            ErrorCategory::ImageTypeNotSupported => 400,

            ErrorCategory::AuthorizationRequired => 401,
            ErrorCategory::LicenseError => 402,
            ErrorCategory::ActionForbidden => 403,
            ErrorCategory::PrimaryResourceNotFound => 404,

            ErrorCategory::SecondaryResourceNotFound |
            ErrorCategory::InternalError |
            ErrorCategory::Unknown |
            ErrorCategory::NoSolutionFound |
            ErrorCategory::IoError => 500,

            ErrorCategory::UpstreamError => 502,
            ErrorCategory::OutOfMemory => 503,
            ErrorCategory::UpstreamTimeout => 504,
        }
    }
    pub fn from_c_status_code(status: i32) -> ErrorCategory{
        match status{
            0 => ErrorCategory::Ok,
            10 => ErrorCategory::OutOfMemory,
            20 => ErrorCategory::IoError,
            30 | 40 | 50 | 51 | 52 | 53 | 54 | 61 => ErrorCategory::InternalError,
            60 => ErrorCategory::ImageMalformed,
            c if c > 200 && c <= ErrorCategory::LicenseError as i32 + 200 => unsafe { ::std::mem::transmute(c - 200) },
            other => ErrorCategory::Unknown
        }
    }
    pub fn to_c_status_code(&self) -> i32{
        match *self{
            ErrorCategory::Ok => 0,
            ErrorCategory::Unknown => 1024,
            other => 200 + *self as i32
        }
    }

    pub fn to_imageflow_category_code(&self) -> i32{
        *self as i32
    }
}

pub struct OutwardErrorBuffer{
    category: ErrorCategory,
    last_panic: Option<Box<Any>>,
    last_error: Option<NodeError>
}
impl OutwardErrorBuffer{
    pub fn new() -> OutwardErrorBuffer{
        OutwardErrorBuffer{
            category: ErrorCategory::Ok,
            last_error: None,
            last_panic: None
        }
    }
    pub fn try_set_panic_error(&mut self, value: Box<Any>) -> bool{
        if self.last_panic.is_none() {
            self.category = ErrorCategory::InternalError;
            self.last_panic = Some(value);
            true
        }else{
            false
        }
    }
    pub fn try_set_error(&mut self, error: NodeError) -> bool{
        if self.last_error.is_none() {
            // TODO: self.category = error.category();
            self.category = ErrorCategory::InternalError;
            self.last_error = Some(error);
            true
        }else{
            false
        }

    }
    pub fn has_error(&self) -> bool{
        self.category != ErrorCategory::Ok
    }

    pub fn category(&self) -> ErrorCategory{
        self.category
    }
    pub fn recoverable(&self) -> bool {
        if let Some(ref e) = self.last_error {
            if self.last_panic.is_none() && e.recoverable() {
                true
            } else {
                false
            }
        } else {
            true
        }
    }

    pub fn try_clear(&mut self) -> bool {
        if self.recoverable() {
            self.last_error = None;
            self.category = ErrorCategory::Ok;
            true
        } else {
            false
        }
    }
    pub fn get_buffer_writer(&self) -> writing_to_slices::NonAllocatingFormatter<&Self>{
        writing_to_slices::NonAllocatingFormatter(self)
    }
}

fn format_panic_value(e: &Any, f: &mut std::fmt::Formatter) -> std::fmt::Result {
    if let Some(str) = e.downcast_ref::<String>(){
        write!(f, "panicked: {}\n",str)?;
    }else if let Some(str) = e.downcast_ref::<&str>(){
        write!(f, "panicked: {}\n",str)?;
    }
    Ok(())
}

impl std::fmt::Display for OutwardErrorBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.category != ErrorCategory::Ok{
            write!(f, "{:?}: ", self.category)?;
        }
        if self.last_error.is_some() && self.last_panic.is_some(){
            write!(f, "2 errors:\n")?;
        }

        if let Some(ref panic) = self.last_panic{
            format_panic_value(panic,f)?;
        }
        if let Some(ref error) = self.last_error{
            writeln!(f, "{}", error)?;
        }
        Ok(())
    }
}


#[derive(Debug, Clone, PartialEq)]
pub struct FlowErr {
    pub code: i32,
    pub message_and_stack: String,
}





#[repr(C)]
#[derive(Copy,Clone,Debug, PartialEq)]
pub enum FlowStatusCode {
    NoError = 0,
    OutOfMemory = 10,
    IOError = 20,
    InvalidInternalState = 30,
    NotImplemented = 40,
    InvalidArgument = 50,
    NullArgument = 51,
    InvalidDimensions = 52,
    UnsupportedPixelFormat = 53,
    ItemDoesNotExist = 54,

    ImageDecodingFailed = 60,
    ImageEncodingFailed = 61,


    OtherError = 1024,
    // FIXME: FirstUserDefinedError is 1025 in C but it conflicts with __LastLibraryError
    // ___LastLibraryError,
    FirstUserDefinedError = 1025,
    LastUserDefinedError = 2147483647,
}


#[derive(Debug, PartialEq, Clone)]
pub enum FlowError {
    GraphCyclic,
    Oom,
    Err(FlowErr),
    ErrNotImpl,
    NodeError(NodeError)
}

impl From<NodeError> for FlowError{
    fn from(e: NodeError) -> Self{
        FlowError::NodeError(e)
    }
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let &FlowError::NodeError(ref e) = self {
            write!(f, "{}", e)
        } else {
            write!(f, "{:#?}", self)
        }

    }
}


impl std::error::Error for FlowError{
    fn description(&self) -> &str{
        "std::error::Error for FlowError not implemented"
    }
}

impl FlowError{
    pub fn to_cow(&self) -> Cow<'static, str> {
        match *self {
            FlowError::Err(ref e) => {
                Cow::from(format!("Error {} {}\n", e.code, e.message_and_stack))
            }
            ref other => {
                Cow::from(format!("{:?}", other))
            }
        }
    }
    pub fn panic_time(&self){
        panic!("{}",self.to_cow());
    }
    pub fn panic_with(&self, message: &str){
        panic!("{}\n{}", message, self.to_cow());
    }

    pub fn write_to_buf(&self, buf: &mut ::context::ErrorBuffer) -> bool{
        buf.abi_raise_error_c_style(FlowStatusCode::NotImplemented as i32, None, None, None, None)
    }
    pub unsafe fn write_to_context_ptr(&self, c: *const Context) {
        self.write_to_buf(&mut *(&*c).error_mut());
    }
}





#[derive(Debug,  Clone, PartialEq)]
pub enum ErrorKind{
    GraphCyclic,
    ContextInvalid,
    Oom,
    ErrNotImpl,
    NullArgument,
    InvalidArgument,
    FailedBorrow,
    AllocationFailed,
    NodeParamsMismatch,
    BitmapPointerNull,
    InvalidCoordinates,
    InvalidNodeParams,
    MethodNotImplemented,
    ValidationNotImplemented,
    InvalidNodeConnections,
    InvalidOperation,
    InvalidState,
    CError(FlowErr)

}
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CodeLocation{
    pub line: u32,
    pub column: u32,
    pub file: &'static str,
    pub module: &'static str
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeError{
    pub kind: ErrorKind,
    pub message: String,
    pub at: ::smallvec::SmallVec<[CodeLocation;4]>,
    pub node: Option<::flow::definitions::NodeDebugInfo>
}


impl ::std::error::Error for NodeError {
    fn description(&self) -> &str {
        if self.message.is_empty() {
            "Node Error (no message)"
        }else{
            &self.message
        }
    }

}
impl NodeError{

    pub fn at(mut self, c: CodeLocation ) -> NodeError {
        self.at.push(c);
        self
    }
    pub fn recoverable(&self) -> bool{
        false
    }
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.message.is_empty() {
            write!(f, "Error {:?}: at\n", self.kind)?;
        }else{
            write!(f, "{} at\n", self.message)?;
        }
        let url = if::imageflow_types::build_env_info::BUILT_ON_CI{
            let repo = ::imageflow_types::build_env_info::BUILD_ENV_INFO.get("CI_REPO").unwrap_or(&Some("imazen/imageflow")).unwrap_or("imazen/imageflow");
            let commit =  ::imageflow_types::build_env_info::GIT_COMMIT;
            Some(format!("https://github.com/{}/blob/{}/", repo, commit))
        }else { None };

        for recorded_frame in &self.at{
            write!(f, "{}:{}:{} in {}\n", recorded_frame.file, recorded_frame.line, recorded_frame.column, recorded_frame.module)?;

            if let Some(ref url) = url{
                write!(f, "{}{}#L{}\n",url, recorded_frame.file, recorded_frame.line)?;
            }
        }
        if let Some(ref n) = self.node{
            write!(f, "Active node:\n{:#?}\n", n)?;
        }
        Ok(())
    }
}




pub mod writing_to_slices {
    use ::std;
    use ::std::fmt;
    use ::std::any::Any;
    use ::std::io::Write;
    use ::std::io;
    use ::std::cmp;
    use ::num::FromPrimitive;

    #[derive(Debug)]
    pub enum WriteResult {
        AllWritten(usize),
        TruncatedAt(usize),
        Error { bytes_written: usize, error: std::io::Error }
    }

    impl WriteResult {
        pub fn from(bytes_written: usize, result: std::io::Result<()>) -> WriteResult {
            let error_kind = result.as_ref().map_err(|e| e.kind()).err();
            match error_kind {
                Some(std::io::ErrorKind::WriteZero) => WriteResult::TruncatedAt(bytes_written),
                Some(error) => WriteResult::Error { bytes_written: bytes_written, error: result.unwrap_err() },
                None => WriteResult::AllWritten(bytes_written)
            }
        }
        pub fn bytes_written(&self) -> usize {
            match self {
                &WriteResult::AllWritten(v) => v,
                &WriteResult::TruncatedAt(v) => v,
                &WriteResult::Error { bytes_written, .. } => bytes_written
            }
        }
        pub fn is_ok(&self) -> bool {
            if let &WriteResult::AllWritten(_) = self {
                true
            } else {
                false
            }
        }
    }

    pub struct NonAllocatingFormatter<T>(pub T) where T: std::fmt::Display;

    impl<T> NonAllocatingFormatter<T> where T: std::fmt::Display {
        pub unsafe fn write_and_write_errors_to_cstring(&self, buffer: *mut u8, buffer_length: usize, append_when_truncated: Option<&str>) -> WriteResult {
            let mut slice = ::std::slice::from_raw_parts_mut(buffer, buffer_length);
            self.write_and_write_errors_to_cstring_slice(&mut slice, append_when_truncated)
        }

        pub fn write_to_slice(&self, buffer: &mut [u8]) -> WriteResult {
            let mut cursor = NonAllocatingCursor::new(buffer);
            let result = write!(&mut cursor, "{}", self.0);
            WriteResult::from(cursor.position(), result)
        }

        /// if returned boolean is true, then truncation occurred.
        pub fn write_and_write_errors_to_slice(&self, buffer: &mut [u8], append_when_truncated: Option<&str>) -> WriteResult {
            let capacity = buffer.len();
            let reserve_bytes = append_when_truncated.map(|s| s.len()).unwrap_or(0);
            if reserve_bytes >= capacity {
                WriteResult::TruncatedAt(0)
            } else {
                match self.write_to_slice(&mut buffer[..capacity - reserve_bytes]) {
                    WriteResult::Error { bytes_written, error } => {
                        let mut cursor = NonAllocatingCursor::new(&mut buffer[bytes_written..]);
                        let _ = write!(&mut cursor, "\nerror serialization failed: {:#?}\n", error);
                        WriteResult::Error { bytes_written: cursor.position(), error: error }
                    },
                    WriteResult::TruncatedAt(bytes_written) if append_when_truncated.is_some() => {
                        let mut cursor = NonAllocatingCursor::new(&mut buffer[bytes_written..]);
                        let _ = write!(&mut cursor, "{}", append_when_truncated.unwrap());
                        WriteResult::TruncatedAt(cursor.position())
                    },
                    other => other
                }
            }
        }

        pub fn write_and_write_errors_to_cstring_slice(&self, buffer: &mut [u8], append_when_truncated: Option<&str>) -> WriteResult {
            let capacity = buffer.len();
            if capacity < 2 {
                WriteResult::TruncatedAt(0)
            } else {
                let result = self.write_and_write_errors_to_slice(&mut buffer[..capacity - 1], append_when_truncated);
                //Remove null characters
                for byte in buffer[..result.bytes_written()].iter_mut() {
                    if *byte == 0 {
                        *byte = 32; //spaces??
                    }
                }
                // Add null terminating character
                buffer[result.bytes_written()] = 0;
                result
            }
        }
    }


    /// Unlike io::Cursor, this does not box (allocate) a WriteZero error result
    #[derive(Debug)]
    struct NonAllocatingCursor<'a> {
        inner: &'a mut [u8],
        pos: u64
    }

    impl<'a> NonAllocatingCursor<'a> {
        pub fn new(buffer: &'a mut [u8]) -> NonAllocatingCursor<'a> {
            NonAllocatingCursor {
                inner: buffer,
                pos: 0
            }
        }
        pub fn position(&self) -> usize {
            cmp::min(usize::from_u64(self.pos).expect("Error serialization cursor has exceeded 2GB"), self.inner.len())
        }
    }

    impl<'a> Write for NonAllocatingCursor<'a> {
        #[inline]
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            let pos = cmp::min(self.pos, self.inner.len() as u64);
            let amt = (&mut self.inner[(pos as usize)..]).write(data)?;
            self.pos += amt as u64;
            Ok(amt)
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }

        fn write_all(&mut self, mut buf: &[u8]) -> io::Result<()> {
            while !buf.is_empty() {
                match self.write(buf) {
                    Ok(0) => return Err(io::Error::from(io::ErrorKind::WriteZero)),
                    Ok(n) => buf = &buf[n..],
                    Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
    }


    #[test]
    fn test_write_cstr() {

        let a = NonAllocatingFormatter("hello");

        let mut large = [0u8; 100];

        assert!(a.write_and_write_errors_to_cstring_slice(&mut large, None).is_ok());
        assert_eq!(b"hello\0"[..], large[..6]);



        let mut small = [0u8; 5];

        let result = a.write_and_write_errors_to_cstring_slice(&mut small, None);
        assert!(result.is_ok() == false);
        assert_eq!(result.bytes_written(), 4);

    }
}

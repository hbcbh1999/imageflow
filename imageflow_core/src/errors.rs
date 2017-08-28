use ::std;
use ::std::fmt;
use ::context::Context;
use std::borrow::Cow;
use std::any::Any;
use std::io::Write;
use std::io;
use std::cmp;
use num::FromPrimitive;
use ::ffi;
use std::ffi::CStr;
use std::ptr;

#[macro_export]
macro_rules! here {
    () => (
        ::CodeLocation{ line: line!(), column: column!(), file: file!()}
    );
}
#[macro_export]
macro_rules! loc {
    () => (
        concat!(file!(), ":", line!(), ":", column!())
    );
    ($msg:expr) => (
        concat!($msg, " at\n", file!(), ":", line!(), ":", column!())
    );
}
#[macro_export]
macro_rules! nerror {
    ($kind:expr) => (
        ::NodeError{
            kind: $kind,
            message: format!("NodeError {:?}", $kind),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
    ($kind:expr, $fmt:expr) => (
        ::NodeError{
            kind: $kind,
            message:  format!(concat!("NodeError {:?}: ",$fmt ), $kind,),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
    ($kind:expr, $fmt:expr, $($arg:tt)*) => (
        ::NodeError{
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
        ::NodeError{
            kind: ::ErrorKind::MethodNotImplemented,
            message: String::new(),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
}


#[macro_export]
macro_rules! cerror {
    ($context:expr) => {{
        let cerr = $ context.c_error().require();
        ::NodeError{
            kind: ::ErrorKind::CError(cerr.status()),
            message: cerr.into_string(),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here ! ())
    }};
    ($context:expr, $fmt:expr) => {{
        let cerr = $context.c_error().require();
        ::NodeError{
            kind: ::ErrorKind::CError(cerr.status()),
            message: format!(concat!($fmt, ": {}"), cerr.into_string()),
            at: ::smallvec::SmallVec::new(),
            node:None
        }.at(here ! ())
    }};
    ($context:expr, $fmt:expr, $($arg:tt)*) => {{
        let cerr = $context.c_error().require();
        ::NodeError{
            kind: ::ErrorKind::CError(cerr.status()),
            message: format!(concat!($fmt, ": {}"), $($arg)*, cerr.into_string()),
            at: ::smallvec::SmallVec::new(),
            node:None
        }.at(here ! ())
    }};
}

#[macro_export]
macro_rules! err_oom {
    () => (
        ::NodeError{
            kind: ::ErrorKind::AllocationFailed,
            message: String::new(),
            at: ::smallvec::SmallVec::new(),
            node: None
        }.at(here!())
    );
}

pub type Result<T> = std::result::Result<T, NodeError>;

trait CategorizedError{
    fn category(&self) -> ErrorCategory;
}

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
    Unknown,
    /// A custom error defined by a third-party plugin
    Custom

    // !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
    // NOTE - safe use of transmute in from_i32 requires that there be no numbering gaps in this list
    // Also keep ErrorCategory::last() up-to-date
    // !!!!!!!!!!!!!!!!!!!!!!!!!!
}
impl ErrorCategory{

    pub fn last() -> ErrorCategory {
        ErrorCategory::Unknown
    }
    fn from_i32(v: i32) -> Option<ErrorCategory>{
        if v >= 0 && v <= ErrorCategory::last() as i32 {
            Some( unsafe { ::std::mem::transmute(v) })
        }else {
            None
        }

    }

    pub fn from_c_status(status: i32) -> Option<ErrorCategory>{
        if let Some(v) = ErrorCategory::from_i32(status - 200){
            Some(v)
        }else {
            match status {
                0 => Some(ErrorCategory::Ok),
                10 => Some(ErrorCategory::OutOfMemory),
                20 => Some(ErrorCategory::IoError),
                30 | 40 | 50 | 51 | 52 | 53 | 54 | 61 => Some(ErrorCategory::InternalError),
                60 => Some(ErrorCategory::ImageMalformed),
                _ => None
            }
        }
    }

    pub fn to_c_status(&self) -> i32{
        match *self{
            ErrorCategory::Ok => 0,
            ErrorCategory::Custom => 1025,
            ErrorCategory::Unknown => 1024,
            ErrorCategory::OutOfMemory => 10,
            ErrorCategory::IoError => 20,
            ErrorCategory::InternalError => 30,
            ErrorCategory::ImageMalformed => 60,
            other => 200 + *self as i32
        }
    }
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
            ErrorCategory::Custom |
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
            ErrorCategory::Custom |
            ErrorCategory::IoError => 500,

            ErrorCategory::UpstreamError => 502,
            ErrorCategory::OutOfMemory => 503,
            ErrorCategory::UpstreamTimeout => 504,
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
            self.category = error.category();
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
pub struct CError {
    status: CStatus,
    message_and_stack: String
}
impl CategorizedError for CError{
    fn category(&self) -> ErrorCategory {
        self.status().category()
    }
}

impl CError{
    pub fn status(&self) -> CStatus{
        self.status
    }
    pub fn into_string(self) -> String{
        self.message_and_stack
    }
    pub fn new(status: CStatus, message_and_stack: String) -> CError{
        CError{ status: status, message_and_stack: message_and_stack}
    }
    pub fn from_status(status: CStatus) -> CError{
        CError{ status: status, message_and_stack: String::new()}
    }

}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CStatus{
    Custom(i32),
    Unknown(i32),
    ErrorMismatch,
    Cat(ErrorCategory),
}
impl CategorizedError for CStatus{
    fn category(&self) -> ErrorCategory {
        match self{
            &CStatus::Custom(_) => ErrorCategory::Custom,
            &CStatus::Unknown(_) => ErrorCategory::Unknown,
            &CStatus::ErrorMismatch => ErrorCategory::InternalError,
            &CStatus::Cat(c) => c
        }
    }
}
impl From<i32> for CStatus{
    fn from(v: i32) -> CStatus{
        if let Some(cat) = ErrorCategory::from_c_status(v){
            CStatus::Cat(cat)
        }else if v > 1024 {
            CStatus::Custom(v)
        }else if v == 90 {
            CStatus::ErrorMismatch
        }else{
            CStatus::Unknown(v)
        }
    }
}
impl CStatus {
    pub fn to_i32(&self) -> i32{
        match self{
            &CStatus::Custom(v) => v,
            &CStatus::Unknown(v) => v,
            &CStatus::ErrorMismatch => 90,
            &CStatus::Cat(c) => c.to_c_status()
        }
    }
}






#[derive(Debug,  Clone, PartialEq, Eq)]
pub enum ErrorKind{
    AllocationFailed,

    GraphCyclic,
    InvalidNodeConnections,

    NullArgument,
    InvalidArgument,
    InvalidCoordinates,
    InvalidNodeParams,

    FailedBorrow,
    NodeParamsMismatch,
    BitmapPointerNull,
    MethodNotImplemented,
    ValidationNotImplemented,
    InvalidOperation,
    InvalidState,
    Category(ErrorCategory),
    CError(CStatus)
}
impl CategorizedError for ErrorKind{
    fn category(&self) -> ErrorCategory{
        match self{
            &ErrorKind::AllocationFailed => ErrorCategory::OutOfMemory,

            &ErrorKind::GraphCyclic |
            &ErrorKind::InvalidNodeConnections => ErrorCategory::GraphInvalid,
            &ErrorKind::NullArgument |
            &ErrorKind::InvalidArgument |
            &ErrorKind::InvalidCoordinates |
            &ErrorKind::InvalidNodeParams => ErrorCategory::ArgumentInvalid,
            &ErrorKind::FailedBorrow |
            &ErrorKind::NodeParamsMismatch |
            &ErrorKind::BitmapPointerNull |
            &ErrorKind::MethodNotImplemented |
            &ErrorKind::ValidationNotImplemented |
            &ErrorKind::InvalidOperation |
            &ErrorKind::InvalidState => ErrorCategory::InternalError,
            &ErrorKind::CError(ref e) => e.category(),
            &ErrorKind::Category(c) => c
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct CodeLocation{
    pub line: u32,
    pub column: u32,
    pub file: &'static str,
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
        &self.message
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

    pub fn category(&self) -> ErrorCategory{
        self.kind.category()
    }

    pub fn panic(&self) -> !{
        eprintln!("{}", self);
        panic!(format!("{}", self));
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
            write!(f, "{}:{}:{}\n", recorded_frame.file, recorded_frame.line, recorded_frame.column)?;

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


#[derive(Clone,Debug)]
pub struct CErrorProxy {
    c_ctx: *mut ffi::ImageflowContext,
}
impl CErrorProxy {
    pub(crate) fn new(c_context: *mut ffi::ImageflowContext) -> CErrorProxy{
        CErrorProxy{
            c_ctx: c_context
        }
    }
    pub(crate) fn null() -> CErrorProxy{
        CErrorProxy{
            c_ctx: ptr::null_mut()
        }
    }
    pub fn has_error(&self) -> bool{
        unsafe{
            ffi::flow_context_has_error(self.c_ctx)
        }
    }
    pub fn status(&self) -> CStatus{
        unsafe {
            CStatus::from(ffi::flow_context_error_reason(self.c_ctx))
        }
    }
    pub fn require(&self) -> CError{
        let e = self.get();
        if e.status() == CStatus::Cat(ErrorCategory::Ok){
            CError::from_status(CStatus::ErrorMismatch)
        }else {
            e
        }
    }
    pub fn get(&self) -> CError {
        let status = self.status();

        match status {
            CStatus::Cat(ErrorCategory::OutOfMemory) |
            CStatus::Cat(ErrorCategory::Ok) => CError::from_status(status),
            other => {
                CError::new(other, self.get_error_and_stacktrace())
            }
        }
    }

    fn get_error_and_stacktrace(&self) -> String{
        unsafe {
            let mut buf = vec![0u8; 2048];

            let chars_written =
                ::ffi::flow_context_error_and_stacktrace(self.c_ctx, buf.as_mut_ptr(), buf.len(), false);

            if chars_written < 0 {
                //TODO: Retry until it fits
                panic!("Error msg doesn't fit in 2kb");
            } else {
                buf.resize(chars_written as usize, 0u8);
            }
            String::from_utf8(buf).unwrap()
        }
    }
}

// Unused
impl CErrorProxy{
    fn clear_error(&mut self){
        unsafe {
            ffi::flow_context_clear_error(self.c_ctx)
        }
    }
    /// # Expectations
    ///
    /// * Strings `message` and `function_name`, and `filename` should be null-terminated UTF-8 strings.
    /// * The lifetime of `message` is expected to exceed the duration of this function call.
    /// * The lifetime of `filename` and `function_name` (if provided), is expected to match or exceed the lifetime of `context`.
    /// * You may provide a null value for `filename` or `function_name`, but for the love of puppies,
    /// don't provide a dangling or invalid pointer, that will segfault... a long time later.
    ///
    /// # Caveats
    ///
    /// * You cannot raise a second error until the first has been cleared with
    ///  `imageflow_context_clear_error`. You'll be ignored, as will future
    ///   `imageflow_add_to_callstack` invocations.
    /// * If you provide an error code of zero (why?!), a different error code will be provided.
    fn c_raise_error(&mut self,
                         error_code: i32,
                         message: Option<&CStr>,
                         filename: Option<&'static CStr>,
                         line: Option<i32>,
                         function_name: Option<&'static CStr>)
                         -> bool {
        unsafe {
            ffi::flow_context_raise_error(self.c_ctx, error_code,
                                          message.map(|cstr| cstr.as_ptr()).unwrap_or(ptr::null()),
                                          filename.map(|cstr| cstr.as_ptr()).unwrap_or(ptr::null()),
                                          line.unwrap_or(-1),
                                          function_name.map(|cstr| cstr.as_ptr()).unwrap_or(ptr::null()))
        }
    }

    fn c_add_to_callstack(&mut self,
                              filename: Option<&'static CStr>,
                              line: Option<i32>,
                              function_name: Option<&'static CStr>)
                              -> bool {
        unsafe {
            ffi::flow_context_add_to_callstack(self.c_ctx,
                                               filename.map(|cstr| cstr.as_ptr()).unwrap_or(ptr::null()),
                                               line.unwrap_or(-1),
                                               function_name.map(|cstr| cstr.as_ptr()).unwrap_or(ptr::null()))
        }
    }

}

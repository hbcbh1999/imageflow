
#ifndef cheddar_generated_imageflow_default_h
#define cheddar_generated_imageflow_default_h


#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>


struct imageflow_context;
struct imageflow_json_response;
struct imageflow_job;
struct imageflow_job_io;
        

///
/// What is possible with the IO object
typedef enum imageflow_io_mode {
	imageflow_io_mode_none = 0,
	imageflow_io_mode_read_sequential = 1,
	imageflow_io_mode_write_sequential = 2,
	imageflow_io_mode_read_seekable = 5,
	imageflow_io_mode_write_seekable = 6,
	imageflow_io_mode_read_write_seekable = 15,
} imageflow_io_mode;

///
/// Input or output?
typedef enum imageflow_direction {
	imageflow_direction_out = 8,
	imageflow_direction_in = 4,
} imageflow_direction;

///
/// When a resource should be closed/freed/cleaned up
///
typedef enum imageflow_cleanup_with {
	/// When the context is destroyed
	imageflow_cleanup_with_context = 0,
	/// When the first job that the item is associated with is destroyed. (Not yet implemented)
	imageflow_cleanup_with_first_job = 1,
} imageflow_cleanup_with;

///
/// How long the provided pointer/buffer will remain valid.
/// Callers must prevent the memory from being freed or moved until this contract expires.
///
typedef enum imageflow_lifetime {
	/// Pointer will outlive function call. If the host language has a garbage collector, call the appropriate method to ensure the object pointed to will not be collected or moved until the call returns. You may think host languages do this automatically in their FFI system. Most do not.
	imageflow_lifetime_outlives_function_call = 0,
	/// Pointer will outlive context. If the host language has a GC, ensure that you are using a data type guaranteed to neither be moved or collected automatically.
	imageflow_lifetime_outlives_context = 1,
} imageflow_lifetime;

/// Creates and returns an imageflow context.
/// An imageflow context is required for all other imageflow API calls.
///
/// An imageflow context tracks
/// * error state
/// * error messages
/// * stack traces for errors (in C land, at least)
/// * context-managed memory allocations
/// * performance profiling information
///
/// **Contexts are not thread-safe!** Once you create a context, *you* are responsible for ensuring that it is never involved in two overlapping API calls.
///
/// Returns a null pointer if allocation fails.
struct imageflow_context* imageflow_context_create(void);

/// Begins the process of destroying the context, yet leaves error information intact
/// so that any errors in the tear-down process can be
/// debugged with imageflow_context_error_and_stacktrace.
///
/// Returns true if no errors occurred. Returns false if there were tear-down issues.
///
/// *Behavior is undefined if context is a null or invalid ptr.*
bool imageflow_context_begin_terminate(struct imageflow_context* context);

/// Destroys the imageflow context and frees the context object.
/// Only use this with contexts created using imageflow_context_create
///
/// Behavior is undefined if context is a null or invalid ptr; may segfault on free(NULL);
void imageflow_context_destroy(struct imageflow_context* context);

/// Returns true if the context is in an error state. You must immediately deal with the error,
/// as subsequent API calls will fail or cause undefined behavior until the error state is cleared
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
bool imageflow_context_has_error(struct imageflow_context* context);

/// Returns true if the context is "ok" or in an error state that is recoverable.
/// You must immediately deal with the error,
/// as subsequent API calls will fail or cause undefined behavior until the error state is cleared
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
bool imageflow_context_error_recoverable(struct imageflow_context* context);

/// Returns true if the context is "ok" or in an error state that is recoverable.
/// You must immediately deal with the error,
/// as subsequent API calls will fail or cause undefined behavior until the error state is cleared
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
bool imageflow_context_error_try_clear(struct imageflow_context* context);

/// Prints the error messages and stacktrace to the given buffer in UTF-8 form; writes a null
/// character to terminate the string, and *ALSO* returns the number of bytes written.
///
///
/// Happy(ish) path: Returns the length of the error message written to the buffer.
/// Sad path: Returns -1 if buffer_length was too small or buffer was nullptr.
/// full_file_path, if true, will display the directory associated with the files in each stack frame.
///
/// Please be accurate with the buffer length, or a buffer overflow will occur.
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
int64_t imageflow_context_error_and_stacktrace(struct imageflow_context* context, char* buffer, size_t buffer_length, bool full_file_path);

/// Prints the error messages (and optional stack frames) to the given buffer in UTF-8 form; writes a null
/// character to terminate the string, and *ALSO* provides the number of bytes written (excluding the null terminator)
///
/// Returns false if the buffer was too small (or null) and the output was truncated.
/// Returns true if all data was written OR if there was a bug in error serialization (that gets written, too).
///
/// If the data is truncated, "\n[truncated]\n" is written to the buffer
///
/// Please be accurate with the buffer length, or a buffer overflow will occur.
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
bool imageflow_context_error_write_to_buffer(struct imageflow_context* context, char* buffer, size_t buffer_length, size_t* bytes_written);

/// Returns the numeric code associated with the error.
///
/// ## Error categories
///
/// * 0 - No error condition.
///
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
int32_t imageflow_context_error_code(struct imageflow_context* context);

/// Prints the error to stderr and exits the process if an error has been raised on the context.
/// If no error is present, the function returns false.
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
///
/// THIS PRINTS DIRECTLY TO STDERR! Do not use in any kind of service! Command-line usage only!
bool imageflow_context_print_and_exit_if_error(struct imageflow_context* context);

///
/// Writes fields from the given imageflow_json_response to the locations referenced.
/// The buffer pointer sent out will be a UTF-8 byte array of the given length (not null-terminated). It will
/// also become invalid if the struct imageflow_json_response associated is freed, or if the context is destroyed.
///
bool imageflow_json_response_read(struct imageflow_context* context, struct imageflow_json_response const* response_in, int64_t* status_code_out, uint8_t const** buffer_utf8_no_nulls_out, size_t* buffer_size_out);

/// Frees memory associated with the given object (and owned objects) after
/// running any owned or attached destructors. Returns false if something went wrong during tear-down.
///
/// Returns true if the object to destroy is a null pointer, or if tear-down was successful.
///
/// Behavior is undefined if the pointer is dangling or not a valid memory reference.
/// Although certain implementations catch
/// some kinds of invalid pointers, a segfault is likely in future revisions).
///
/// Behavior is undefined if the context provided does not match the context with which the
/// object was created.
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
///
bool imageflow_json_response_destroy(struct imageflow_context* context, struct imageflow_json_response* response);

///
/// Sends a JSON message to the imageflow_context
///
/// The context is provided `method`, which determines which code path will be used to
/// process the provided JSON data and compose a response.
///
/// * `method` and `json_buffer` are only borrowed for the duration of the function call. You are
///    responsible for their cleanup (if necessary - static strings are handy for things like
///    `method`).
/// * `method` should be a UTF-8 null-terminated string.
///   `json_buffer` should be a UTF-8 encoded buffer (not null terminated) of length json_buffer_size.
///
/// You should call `imageflow_context_has_error()` to see if this succeeded.
///
/// A struct imageflow_json_response is returned for success and most error conditions.
/// Call `imageflow_json_response_destroy` when you're done with it (or dispose the context).
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
struct imageflow_json_response const* imageflow_context_send_json(struct imageflow_context* context, char const* method, uint8_t const* json_buffer, size_t json_buffer_size);

///
/// Sends a JSON message to the imageflow_job
///
/// The recipient is provided `method`, which determines which code path will be used to
/// process the provided JSON data and compose a response.
///
/// * `method` and `json_buffer` are only borrowed for the duration of the function call. You are
///    responsible for their cleanup (if necessary - static strings are handy for things like
///    `method`).
/// * `method` should be a UTF-8 null-terminated string.
///   `json_buffer` should be a UTF-8 encoded buffer (not null terminated) of length json_buffer_size.
///
/// You should call `imageflow_context_has_error()` to see if this succeeded.
///
/// A struct imageflow_json_response is returned for success and most error conditions.
/// Call `imageflow_json_response_destroy` when you're done with it (or dispose the context).
///
/// Behavior is undefined if `context` is a dangling or invalid ptr; segfault likely.
struct imageflow_json_response const* imageflow_job_send_json(struct imageflow_context* context, struct imageflow_job* job, char const* method, uint8_t const* json_buffer, size_t json_buffer_size);

///
/// Creates an imageflow_io object to wrap a filename.
///
/// The filename should be a null-terminated string. It should be written in codepage used by your operating system for handling `fopen` calls.
/// https://msdn.microsoft.com/en-us/library/yeby3zcb.aspx
///
/// If the filename is fopen compatible, you're probably OK.
///
/// As always, `mode` is not enforced except for the file open flags.
///
struct imageflow_job_io* imageflow_io_create_for_file(struct imageflow_context* context, imageflow_io_mode mode, char const* filename, imageflow_cleanup_with cleanup);

///
/// Creates an imageflow_io structure for reading from the provided buffer.
/// You are ALWAYS responsible for freeing the memory provided in accordance with the imageflow_lifetime value.
/// If you specify OutlivesFunctionCall, then the buffer will be copied.
///
///
struct imageflow_job_io* imageflow_io_create_from_buffer(struct imageflow_context* context, uint8_t const* buffer, size_t buffer_byte_count, imageflow_lifetime lifetime, imageflow_cleanup_with cleanup);

///
/// Creates an imageflow_io structure for writing to an expanding memory buffer.
///
/// Reads/seeks, are, in theory, supported, but unless you've written, there will be nothing to read.
///
/// The I/O structure and buffer will be freed with the context.
///
///
/// Returns null if allocation failed; check the context for error details.
struct imageflow_job_io* imageflow_io_create_for_output_buffer(struct imageflow_context* context);

///
/// Provides access to the underlying buffer for the given imageflow_io object.
///
/// Ensure your length variable always holds 64-bits.
///
bool imageflow_io_get_output_buffer(struct imageflow_context* context, struct imageflow_job_io* io, uint8_t const** result_buffer, size_t* result_buffer_length);

///
/// Provides access to the underlying buffer for the given imageflow_io object.
///
/// Ensure your length variable always holds 64-bits
///
bool imageflow_job_get_output_buffer_by_id(struct imageflow_context* context, struct imageflow_job* job, int32_t io_id, uint8_t const** result_buffer, size_t* result_buffer_length);

///
/// Creates an imageflow_job, which permits the association of imageflow_io instances with
/// numeric identifiers and provides a 'sub-context' for job execution
///
struct imageflow_job* imageflow_job_create(struct imageflow_context* context);

///
/// Looks up the imageflow_io pointer from the provided io_id
///
struct imageflow_job_io* imageflow_job_get_io(struct imageflow_context* context, struct imageflow_job* job, int32_t io_id);

///
/// Associates the imageflow_io object with the job and the assigned io_id.
///
/// The io_id will correspond with io_id in the graph
///
/// direction is in or out.
bool imageflow_job_add_io(struct imageflow_context* context, struct imageflow_job* job, struct imageflow_job_io* io, int32_t io_id, imageflow_direction direction);

///
/// Destroys the provided imageflow_job
///
bool imageflow_job_destroy(struct imageflow_context* context, struct imageflow_job* job);

///
/// Allocates zeroed memory that will be freed with the context.
///
/// * filename/line may be used for debugging purposes. They are optional. Provide null/-1 to skip.
/// * `filename` should be an null-terminated UTF-8 or ASCII string which will outlive the context.
///
/// Returns null(0) on failure.
///
void* imageflow_context_memory_allocate(struct imageflow_context* context, size_t bytes, char const* filename, int32_t line);

///
/// Frees memory allocated with imageflow_context_memory_allocate early.
///
/// * filename/line may be used for debugging purposes. They are optional. Provide null/-1 to skip.
/// * `filename` should be an null-terminated UTF-8 or ASCII string which will outlive the context.
///
/// Returns false on failure.
///
bool imageflow_context_memory_free(struct imageflow_context* context, void* pointer, char const* filename, int32_t line);



#ifdef __cplusplus
}
#endif


#endif

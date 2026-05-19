/// Resume execution
pub const NEXT: u32 = 0;
/// Throw an error
pub const THROW: u32 = 1;
/// Return (complete)
pub const RETURN: u32 = 2;
/// Break to label
pub const BREAK: u32 = 3;
/// Yield a value (used for await)
pub const YIELD: u32 = 4;
/// Yield* delegation
pub const YIELD_STAR: u32 = 5;
/// Catch
pub const CATCH: u32 = 6;
/// End finally
pub const END_FINALLY: u32 = 7;

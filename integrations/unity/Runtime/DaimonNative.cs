// DaimonNative.cs — raw P/Invoke bindings for the Daimon cognitive engine C ABI.
//
// This file mirrors crates/daimon-ffi/include/daimon.h EXACTLY (8 functions,
// Cdecl, UTF-8 strings). Do not add logic here — keep it a thin, faithful
// translation of the C header. The wrapper in DaimonAgent.cs owns all memory
// rules (free returned char*, never free version()).
//
// Native library file stem is `daimon_ffi`:
//   macOS   : libdaimon_ffi.dylib   -> DllImport("daimon_ffi") resolves it
//   Linux   : libdaimon_ffi.so      -> DllImport("daimon_ffi") resolves it
//   Windows : daimon_ffi.dll        -> DllImport("daimon_ffi") resolves it
//   iOS     : static lib linked in  -> DllImport("__Internal")  (see #if below)
//
// UTF-8: incoming `const char*` params use [MarshalAs(UnmanagedType.LPUTF8Str)]
// so .NET marshals managed string -> UTF-8. Returned `char*` is declared as
// IntPtr (NOT string) so .NET does not auto-marshal-and-free it with the wrong
// allocator; the caller converts with Marshal.PtrToStringUTF8 and then frees via
// daimon_string_free (except daimon_version, which is a static string).

using System;
using System.Runtime.InteropServices;

namespace Daimon
{
    /// <summary>
    /// 1:1 extern declarations for the Daimon C ABI. Internal — game code should
    /// use the <see cref="DaimonAgent"/> wrapper, which enforces the memory rules.
    /// </summary>
    internal static class DaimonNative
    {
#if UNITY_IOS && !UNITY_EDITOR
        // On iOS the engine must be linked as a STATIC library (.a) into the app
        // binary; symbols are resolved against the executable itself.
        private const string Lib = "__Internal";
#else
        // Desktop + Android + iOS-in-Editor: dynamic library by file stem.
        private const string Lib = "daimon_ffi";
#endif

        // DaimonAgent* daimon_agent_new(const char* persona_json, uint64_t seed)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_agent_new(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string persona_json,
            ulong seed);

        // char* daimon_agent_think(DaimonAgent*, const char* input_json)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_agent_think(
            IntPtr agent,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string input_json);

        // char* daimon_agent_save(DaimonAgent*)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_agent_save(IntPtr agent);

        // DaimonAgent* daimon_agent_load(const char* name, const char* mind_json)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_agent_load(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string mind_json);

        // void daimon_agent_free(DaimonAgent*)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void daimon_agent_free(IntPtr agent);

        // void daimon_string_free(char*)
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void daimon_string_free(IntPtr s);

        // const char* daimon_version(void)  — static, DO NOT free
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_version();

        // char* daimon_last_error(void)  — free it
        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr daimon_last_error();
    }
}

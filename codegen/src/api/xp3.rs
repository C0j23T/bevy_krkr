//! Archives: XP3 (and TAR) through the engine's archive openers (base/XP3Archive.cpp,
//! base/win32/StorageImpl.cpp TVPOpenArchive).

use super::KirikiridModule::XP3Archive;
use super::{cstr, p, FnDecl, ARCHIVE};

pub const FUNCTIONS: &[FnDecl] = &[
    FnDecl {
        module: XP3Archive,
        name: "krkr_xp3_open",
        doc: "Open an archive on the host filesystem (XP3 or TAR). Requires krkr_engine_init. \
              Returns NULL on failure.",
        c_ret: "XP3ArchiveOpaque*",
        rust_ret: "*mut XP3ArchiveOpaque",
        params: &[cstr("path")],
        cpp_target: "TVPOpenArchive",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(path);
    std::string native = krkr_host_path(path);
    tTVPArchive *arc = TVPOpenArchive(TVPNormalizeStorageName(krkr_ttstr(native.c_str())), true);
    if (!arc) throw std::runtime_error(std::string("not a supported archive: ") + path);
    return new XP3ArchiveOpaque{arc};"#,
    },
    FnDecl {
        module: XP3Archive,
        name: "krkr_xp3_close",
        doc: "Close an archive from krkr_xp3_open.",
        c_ret: "void",
        rust_ret: "",
        params: &[ARCHIVE],
        cpp_target: "tTVPArchive::Release",
        cpp_body: r#"
    if (!archive) return;
    if (archive->arc) archive->arc->Release();
    delete archive;"#,
    },
    FnDecl {
        module: XP3Archive,
        name: "krkr_xp3_read_file",
        doc: "Read archive member `name` into a new buffer; free it with krkr_free.",
        c_ret: "krkr_result_t",
        rust_ret: "krkr_result_t",
        params: &[
            ARCHIVE,
            cstr("name"),
            p("out_data", "uint8_t**", "*mut *mut u8"),
            p("out_len", "size_t*", "*mut usize"),
        ],
        cpp_target: "tTVPArchive::IsExistent + tTVPArchive::CreateStream",
        cpp_body: r#"
    krkr_require_running();
    krkr_require(archive && name && out_data && out_len);
    *out_data = nullptr;
    *out_len = 0;
    ttstr member = krkr_ttstr(name);
    if (!archive->arc->IsExistent(member)) throw std::runtime_error(std::string("no such archive member: ") + name);
    std::unique_ptr<tTJSBinaryStream> stream(archive->arc->CreateStream(member));
    if (!stream) throw std::runtime_error(std::string("cannot open archive member: ") + name);
    tjs_uint64 size = stream->GetSize();
    std::unique_ptr<uint8_t, decltype(&std::free)> buf((uint8_t *)std::malloc(size ? size : 1), &std::free);
    if (!buf) throw std::bad_alloc();
    if (size) stream->ReadBuffer(buf.get(), (tjs_uint)size);
    *out_len = (size_t)size;
    *out_data = buf.release();
    return KRKR_OK;"#,
    },
];

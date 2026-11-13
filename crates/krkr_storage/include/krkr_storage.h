#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
// Handles are exclusive and closed exactly once. Buffers must be valid for their
// lengths; null is allowed only for zero bytes. Errors never unwind across ABI.
typedef struct KrkrFile KrkrFile;
typedef struct KrkrDirectory KrkrDirectory;
typedef struct KrkrFsMetadata { uint32_t kind; uint64_t size; int64_t accessed, modified, changed; } KrkrFsMetadata;
// Status: 0 success, -1 failure; reads/writes: SIZE_MAX failure; seek/size:
// UINT64_MAX failure; pointer results: null failure. Seek origin: 0/1/2.
// Modes: 0 read, 1 overwrite, 2 append (create), 3 update (existing). Kind: 1 file, 2 directory, 0 other.
KrkrFile *krkr_fs_open(const char *, uint32_t mode);
void krkr_fs_close(KrkrFile *);
size_t krkr_fs_read(KrkrFile *, void *, size_t);
size_t krkr_fs_write(KrkrFile *, const void *, size_t);
uint64_t krkr_fs_seek(KrkrFile *, int64_t offset, uint32_t whence);
uint64_t krkr_fs_size(KrkrFile *);
int krkr_fs_set_len(KrkrFile *, uint64_t);
int krkr_fs_flush(KrkrFile *);
int krkr_fs_metadata(const char *, KrkrFsMetadata *);
KrkrDirectory *krkr_fs_directory_open(const char *);
// Names are borrowed until directory_close. Null marks the end.
const char *krkr_fs_directory_entry(const KrkrDirectory *, size_t, KrkrFsMetadata *);
void krkr_fs_directory_close(KrkrDirectory *);
int krkr_fs_set_modified(const char *, int64_t seconds);
int krkr_fs_create_dir(const char *, int recursive);
int krkr_fs_remove(const char *, int directory);
int krkr_fs_copy(const char *, const char *, int overwrite);
int krkr_fs_rename(const char *, const char *, int overwrite);
int krkr_fs_replace(const char *, const void *, size_t);
char *krkr_fs_resolve(const char *, int case_insensitive);
char *krkr_fs_writable_root(void);
void krkr_fs_string_free(char *);
// Thread-local message, valid until the next failure on this thread.
const char *krkr_fs_last_error(void);
#ifdef __cplusplus
}
#endif

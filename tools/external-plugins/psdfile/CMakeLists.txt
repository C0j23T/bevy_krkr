cmake_minimum_required(VERSION 3.16)
project(psdfile_alias LANGUAGES CXX)

get_filename_component(REPO_DIR "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)
add_subdirectory("${REPO_DIR}/external_plugins/psdfile" upstream)
target_sources(psdfile PRIVATE "${CMAKE_CURRENT_LIST_DIR}/alias.cpp")
target_compile_definitions(psdfile PRIVATE V2Link=PSDFileLink V2Unlink=PSDFileUnlink)
add_custom_target(psdfile_alias ALL
    COMMAND "${CMAKE_COMMAND}" -E create_symlink
        "$<TARGET_FILE_NAME:psdfile>"
        "$<TARGET_FILE_DIR:psdfile>/${CMAKE_SHARED_LIBRARY_PREFIX}psd${CMAKE_SHARED_LIBRARY_SUFFIX}"
    DEPENDS psdfile)

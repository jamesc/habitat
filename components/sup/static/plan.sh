# shellcheck disable=2034
PLAN_CONTEXT=$(dirname "$PLAN_CONTEXT") source ../plan.sh

pkg_name=hab-sup-static
pkg_maintainer="The Habitat Maintainers <humans@habitat.sh>"
pkg_deps=(core/busybox-static)
pkg_build_deps=(
  core/musl core/zlib-musl core/xz-musl core/bzip2-musl core/libarchive-musl
  core/openssl-musl core/libsodium-musl
  core/coreutils core/cacerts core/rust core/gcc
)

do_begin() {
  # Set the parent directory as the "root" of this plan.
  PLAN_CONTEXT=$(abspath ..)
}

# shellcheck disable=2155
do_prepare() {
  _common_prepare

  export rustc_target="x86_64-unknown-linux-musl"
  build_line "Setting rustc_target=$rustc_target"

  la_ldflags="-L$(pkg_path_for zlib-musl)/lib -lz"
  la_ldflags="$la_ldflags -L$(pkg_path_for xz-musl)/lib -llzma"
  la_ldflags="$la_ldflags -L$(pkg_path_for bzip2-musl)/lib -lbz2"
  la_ldflags="$la_ldflags -L$(pkg_path_for openssl-musl)/lib -lssl -lcrypto"

  export LIBARCHIVE_LIB_DIR=$(pkg_path_for libarchive-musl)/lib
  export LIBARCHIVE_INCLUDE_DIR=$(pkg_path_for libarchive-musl)/include
  export LIBARCHIVE_LDFLAGS="$la_ldflags"
  export LIBARCHIVE_STATIC=true
  export OPENSSL_LIB_DIR=$(pkg_path_for openssl-musl)/lib
  export OPENSSL_INCLUDE_DIR=$(pkg_path_for openssl-musl)/include
  export OPENSSL_STATIC=true
  export SODIUM_LIB_DIR=$(pkg_path_for libsodium-musl)/lib
  export SODIUM_STATIC=true

  # Used to find libgcc_s.so.1 when compiling `build.rs` in dependencies. Since
  # this used only at build time, we will use the version found in the gcc
  # package proper--it won't find its way into the final binaries.
  export LD_LIBRARY_PATH=$(pkg_path_for gcc)/lib
  build_line "Setting LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
}

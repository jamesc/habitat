pkg_name=m4
pkg_derivation=chef
pkg_version=1.4.17
pkg_maintainer="The Bldr Maintainers <bldr@chef.io>"
pkg_license=('gplv3')
pkg_source=http://ftp.gnu.org/gnu/$pkg_name/${pkg_name}-${pkg_version}.tar.xz
pkg_shasum=f0543c3beb51fa6b3337d8025331591e0e18d8ec2886ed391f1aade43477d508
pkg_build_deps=(chef/binutils)
pkg_deps=(chef/glibc)
pkg_binary_path=(bin)
pkg_gpg_key=3853DA6B

do_prepare() {
  # TODO: We need a more clever way to calculate/determine the path to ld-*.so
  LDFLAGS="$LDFLAGS -Wl,-rpath=${LD_RUN_PATH},--enable-new-dtags"
  LDFLAGS="$LDFLAGS -Wl,--dynamic-linker=$(pkg_path_for glibc)/lib/ld-2.22.so"
  export LDFLAGS
  build_line "Updating LDFLAGS=$LDFLAGS"
}

do_build() {
  ./configure --prefix=$pkg_prefix
  make

  if [ -n "${DO_CHECK}" ]; then
    # Fixes a broken test with either gcc 5.2.x and/or perl 5.22.x:
    # FAIL: test-update-copyright.sh
    #
    # Thanks to: http://permalink.gmane.org/gmane.linux.lfs.devel/16285
    sed -i 's/copyright{/copyright\\{/' build-aux/update-copyright

    make check LDFLAGS=""
  fi
}

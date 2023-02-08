pkgname=avif-converter-git
pkgver=1.0.1
pkgrel=1
pkgdesc='Custom avif image converter made with Rust'
arch=('i686' 'x86_64' 'armv7h' 'aarch64')
license=('GPL3')
makedepends=('cargo' 'nasm')

build () {
  cd "$srcdir/avif-converter"

  if [[ $CARCH != x86_64 ]]; then
    export CARGO_PROFILE_RELEASE_LTO=off
  fi

  cargo build --locked --release --target-dir target
}

package() {
  cd "$srcdir/avif-converter"

  install -Dm755 target/release/avif-converter "${pkgdir}/usr/bin/avif-converter"
}

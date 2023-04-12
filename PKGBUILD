pkgname=avif-converter-git
pkgver=1.4.0
pkgrel=1
source=("git+https://git.solstice-x0.arpa/FerrahWolfeh/avif-converter.git")
sha256sums=('SKIP')
pkgdesc='Custom avif image converter made with Rust'
arch=('i686' 'x86_64' 'armv7h' 'aarch64')
license=('GPL3')
makedepends=('cargo' 'nasm')

build () {
  cd "$srcdir/avif-converter"

  if [[ $CARCH != x86_64 ]]; then
    export CARGO_PROFILE_RELEASE_LTO=off
  fi

  cargo build --release --target-dir target
}

package() {
  cd "$srcdir/avif-converter"

  strip target/release/avif-converter

  install -Dm755 target/release/avif-converter "${pkgdir}/usr/bin/avif-converter"
}

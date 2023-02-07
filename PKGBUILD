pkgname=avif-converter-git
pkgver=1.0.0
pkgrel=1
pkgdesc='Custom avif image converter made with Rust'
source=("git://git.solstice-x0.arpa/FerrahWolfeh/avif-converter")
arch=('i686' 'pentium4' 'x86_64' 'arm' 'armv7h' 'armv6h' 'aarch64')
license=('GPL3')
makedepends=('cargo', 'nasm')
sha256sums=('SKIP')

build () {
  cd "$srcdir"

  if [[ $CARCH != x86_64 ]]; then
    export CARGO_PROFILE_RELEASE_LTO=off
  fi

  cargo build --locked --release --target-dir target
}

package() {
  cd "$srcdir"

  install -Dm755 target/release/avif-converter "${pkgdir}/usr/bin/avif-converter"
}

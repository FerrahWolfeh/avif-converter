# Maintainer: Ferrah Aiko <ferrahwolfeh@proton.me>
pkgname=avif-converter
pkgver=1.1.0
pkgrel=1
makedepends=('rust' 'cargo')
arch=('i686' 'x86_64' 'armv6h' 'armv7h')
pkgdesc="Simple tool to batch convert multiple images to AVIF"
license=('GPL3')

build() {
    return 0
}

package() {
    cargo install --root="$pkgdir" avif-converter
}
